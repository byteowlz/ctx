//! Screenshot capture backend for context bundles (macOS-first).
//!
//! Provides the capture modes needed by Omni Agent Handoff:
//! - [`CaptureTarget::Frontmost`]: the focused window.
//! - [`CaptureTarget::Display`]: the current/main display.
//! - [`CaptureTarget::All`]: every display.
//!
//! Capture best-effort provenance (app, window title, URL if supplied, display,
//! mode, rect, timestamp) and writes JPEG images into the bundle. Failures
//! (permissions, no display) degrade gracefully into [`Warning`]s rather than
//! aborting the bundle. The design leaves room for Wayland portal capture and
//! Windows backends later.

use std::fs;
use std::path::{Path, PathBuf};

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use screenshots::Screen;
use time::OffsetDateTime;
use uuid::Uuid;
use x_win::get_active_window;

use crate::manifest::{
    CaptureMode, Dimensions, ImageProvenance, ImageRole, Item, Rect, Warning,
};
use crate::store::{item_id, BundleStoreMut, StoreError};

/// Which target to capture for a bundle screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    /// The frontmost / focused window (cropped from the display it is on).
    Frontmost,
    /// The main display.
    Display,
    /// All displays (one image each).
    All,
}

impl CaptureTarget {
    /// Map to the manifest provenance mode.
    pub fn mode(self) -> CaptureMode {
        match self {
            CaptureTarget::Frontmost => CaptureMode::Frontmost,
            CaptureTarget::Display => CaptureMode::Display,
            CaptureTarget::All => CaptureMode::All,
        }
    }
}

/// Optional hints supplied by the producer (e.g. Omni) when adding a screenshot.
#[derive(Debug, Clone, Default)]
pub struct CaptureHints {
    /// URL of the focused element, if known by the caller.
    pub url: Option<String>,
    /// Optional URL query override for the image quality (0-100).
    pub image_quality: Option<u8>,
}

/// Result of capturing one image for a bundle.
#[derive(Debug, Clone)]
pub struct CapturedImage {
    /// Manifest item that was created.
    pub item: Item,
    /// Pixel dimensions of the written image.
    pub dimensions: Dimensions,
}

/// Add a screenshot item to the bundle using the requested capture target.
///
/// Writes the image to `images/<id>.jpg` inside the bundle. On failure returns
/// an error the caller can translate into a [`Warning`].
pub fn add_screenshot(
    bundle: &mut BundleStoreMut,
    target: CaptureTarget,
    hints: CaptureHints,
) -> Result<Vec<CapturedImage>, StoreError> {
    let now = OffsetDateTime::now_utc();
    let quality = hints.image_quality.unwrap_or(85).clamp(1, 100);
    let mode = target.mode();

    // Determine which displays to capture and, for frontmost, the window rect.
    // Per-display capture plan: (screen, optional window rect, optional app/title).
    type CapturePlan = Vec<(Screen, Option<Rect>, Option<(String, String)>)>;
    let mut captures: CapturePlan = Vec::new();
    let screens = list_screens();

    match target {
        CaptureTarget::All => {
            for screen in &screens {
                captures.push((*screen, None, None));
            }
        }
        CaptureTarget::Display => {
            let primary = screens.first().cloned().ok_or_else(|| {
                StoreError::Other("no displays available for capture".to_string())
            })?;
            captures.push((primary, None, None));
        }
        CaptureTarget::Frontmost => {
            let active = get_active_window().map_err(|e| {
                StoreError::Other(format!("could not determine active window: {e}"))
            })?;
            let app_name = active.info.name.clone();
            let title = active.title.clone();
            let (screen, rect) = find_screen_for_window(&screens, &active)
                .ok_or_else(|| StoreError::Other("active window not on any display".to_string()))?;
            captures.push((screen, Some(rect), Some((app_name, title))));
        }
    }

    let mut results = Vec::new();
    for (screen, rect, app_meta) in captures {
        let raw = screen
            .capture()
            .map_err(|e| StoreError::Other(format!("display capture failed: {e}")))?;
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_vec(raw.width() as u32, raw.height() as u32, raw.to_vec())
                .ok_or_else(|| {
                    StoreError::Other("failed to read capture buffer".to_string())
                })?;
        let dyn_img = DynamicImage::ImageRgba8(buffer);

        // For frontmost, crop to the window rect on that display.
        let final_img = match rect {
            Some(r) => crop_to_window(&dyn_img, &r).unwrap_or(dyn_img.clone()),
            None => dyn_img.clone(),
        };
        let id = item_id("img");
        let rel = format!("images/screenshot-{id}.jpg");
        let dest = bundle.dir().join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        save_jpeg(&final_img, &dest, quality)?;
        let dims = final_img.dimensions();

        let provenance = ImageProvenance {
            app: app_meta.as_ref().map(|(a, _)| a.clone()),
            window_title: app_meta.as_ref().map(|(_, t)| t.clone()),
            url: hints.url.clone(),
            display: Some(screen.display_info.width.to_string()),
            mode: Some(mode),
            rect,
            captured_at: Some(now),
        };
        let item = bundle.add_image_item(
            ImageRole::Screenshot,
            &rel,
            Some(Dimensions {
                width: dims.0,
                height: dims.1,
            }),
            provenance,
            None,
        )?;
        results.push(CapturedImage {
            item,
            dimensions: Dimensions {
                width: dims.0,
                height: dims.1,
            },
        });
    }

    Ok(results)
}

fn list_screens() -> Vec<Screen> {
    Screen::all().unwrap_or_default()
}

/// Find the display containing a window and compute the window rect within that
/// display's coordinate space. `x_win` reports global coordinates; screenshots
/// are per-display, so we translate the window origin into the display frame.
fn find_screen_for_window(
    screens: &[Screen],
    window: &x_win::WindowInfo,
) -> Option<(Screen, Rect)> {
    let wx = window.position.x;
    let wy = window.position.y;
    for screen in screens {
        let info = &screen.display_info;
        let dx = info.x;
        let dy = info.y;
        // Approximate containment: window origin inside this display.
        if wx >= dx && wy >= dy {
            let local_x = (wx - dx).max(0) as u32;
            let local_y = (wy - dy).max(0) as u32;
            let w = window.position.width as u32;
            let h = window.position.height as u32;
            return Some((
                *screen,
                Rect::new(local_x, local_y, w, h),
            ));
        }
    }
    None
}

fn crop_to_window(img: &DynamicImage, rect: &Rect) -> Option<DynamicImage> {
    let (iw, ih) = img.dimensions();
    let valid = rect.x.checked_add(rect.width).is_some_and(|x| x <= iw)
        && rect.y.checked_add(rect.height).is_some_and(|y| y <= ih)
        && rect.width > 0
        && rect.height > 0;
    if !valid {
        return None;
    }
    Some(img.crop_imm(rect.x, rect.y, rect.width, rect.height))
}

fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<(), StoreError> {
    use std::io::Write;
    let rgb = img.to_rgb8();
    let mut file = fs::File::create(path)?;
    let quality = quality.clamp(1, 100);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ColorType::Rgb8.into(),
        )
        .map_err(StoreError::Image)?;
    // Flush to surface permission/write errors early.
    file.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// OCR backend
// ---------------------------------------------------------------------------

/// OCR backend configuration, derived from [`crate::config::OcrConfig`].
#[derive(Debug, Clone)]
pub struct OcrBackend {
    /// Command to run, e.g. `ocrs`.
    pub command: String,
    /// Extra args inserted before the image path.
    pub args: Vec<String>,
}

impl OcrBackend {
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
        }
    }

    /// Run OCR on an image file, returning the recognized text.
    ///
    /// v1 shells out to the configured command (default `ocrs`). We support two
    /// invocation shapes:
    /// 1. `ocrs <image>` (text on stdout), and
    /// 2. `ocrs <image> -o <out>` or `--out` is not assumed; we prefer stdout.
    pub fn run(&self, image_path: &Path) -> Result<String, OcrError> {
        let mut cmd = std::process::Command::new(&self.command);
        cmd.args(&self.args);
        cmd.arg(image_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let output = cmd.output().map_err(|e| OcrError::Spawn {
            command: self.command.clone(),
            source: e,
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Err(OcrError::NonZeroExit {
                code: output.status.code(),
                stderr,
                stdout,
            });
        }
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(text)
    }
}

/// Errors from the OCR backend.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    /// The OCR command could not be spawned (missing binary / permission).
    #[error("failed to spawn OCR command '{command}': {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    /// The OCR command exited non-zero.
    #[error(
        "OCR command exited with code {code:?}: {stderr}{stdout}",
        stderr = if stderr.is_empty() { String::new() } else { format!(" stderr: {stderr}") },
        stdout = if stdout.is_empty() { String::new() } else { format!(" stdout: {stdout}") }
    )]
    NonZeroExit {
        code: Option<i32>,
        stderr: String,
        stdout: String,
    },
}

/// Run OCR on an image item in the bundle, attaching an OCR text item.
///
/// On failure, records a structured warning on the bundle instead of returning
/// an error, so the bundle stays usable. Returns the OCR text item on success.
pub fn ocr_item(
    bundle: &mut BundleStoreMut,
    image_item_id: &str,
    backend: &OcrBackend,
) -> Result<Option<Item>, StoreError> {
    let manifest = bundle.manifest()?;
    let image_item = manifest
        .find_item(image_item_id)
        .ok_or_else(|| StoreError::ItemNotFound(image_item_id.to_string()))?;
    let path = match &image_item.body {
        crate::manifest::ItemBody::Image(f) => &f.path,
        _ => return Err(StoreError::NotAnImage(image_item_id.to_string())),
    };
    let abs = bundle.dir().join(path);
    if !abs.exists() {
        let _ = bundle.add_warning(
            Warning::new("ocr_failed", format!("image not found: {}", abs.display()))
                .for_item(image_item_id),
        );
        return Ok(None);
    }
    match backend.run(&abs) {
        Ok(text) => {
            let item = bundle.attach_ocr(image_item_id, text.trim())?;
            Ok(Some(item))
        }
        Err(err) => {
            let _ = bundle.add_warning(
                Warning::new("ocr_failed", err.to_string()).for_item(image_item_id),
            );
            Ok(None)
        }
    }
}

/// Read the current clipboard text, if available. Used for the
/// `add-text --role clipboard` workflow.
pub fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
}

/// Capture a desktop snapshot envelope into `path` (JSON), returning the path.
/// Reuses the existing platform capture to produce a full desktop context file.
pub fn write_desktop_snapshot(
    path: &Path,
    capture_dir: &Path,
) -> Result<PathBuf, StoreError> {
    use crate::platform::{CaptureRequest, ContextProvider, DesktopPlatform};
    let request = CaptureRequest {
        capture_dir: capture_dir.to_path_buf(),
        include_clipboard: true,
        include_screenshots: true,
        include_accessibility: true,
        include_actions: false,
        accessibility_depth: 3,
        image_quality: 85,
        screenshot_timeout_ms: 2000,
    };
    let provider = DesktopPlatform::default();
    let result = provider
        .capture(&request)
        .map_err(|e| StoreError::Other(format!("desktop capture failed: {e}")))?;
    let envelope = crate::capture::CaptureEnvelope::new(result);
    let payload = serde_json::to_vec_pretty(&envelope)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, payload)?;
    let _ = Uuid::new_v4();
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Producer;
    use crate::store::BundleStore;

    #[test]
    fn ocr_error_messages_are_actionable() {
        let backend = OcrBackend::new("definitely-not-a-real-ocr-binary-xyz", vec![]);
        let err = backend
            .run(Path::new("irrelevant.png"))
            .expect_err("missing binary should error");
        match err {
            OcrError::Spawn { command, .. } => {
                assert!(command.contains("definitely-not-a-real"));
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn ocr_item_records_warning_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (_manifest, mut h) = store.create(None, Producer::new("ctx")).unwrap();

        // Add an image item with real bytes on disk.
        let rel = "images/x.jpg";
        let abs = h.dir().join(rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        let img = image::RgbImage::from_pixel(10, 10, image::Rgb([1, 2, 3]));
        image::DynamicImage::ImageRgb8(img)
            .save_with_format(&abs, image::ImageFormat::Jpeg)
            .unwrap();
        let image_item = h
            .add_image_item(
                ImageRole::Screenshot,
                rel,
                Some(Dimensions {
                    width: 10,
                    height: 10,
                }),
                ImageProvenance::default(),
                None,
            )
            .unwrap();

        let backend = OcrBackend::new("definitely-not-a-real-ocr-binary-xyz", vec![]);
        let out = ocr_item(&mut h, &image_item.id, &backend).unwrap();
        assert!(out.is_none(), "OCR should fail gracefully");

        // A warning should be recorded on the bundle.
        let loaded = store.load(h.id()).unwrap();
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.warnings[0].code, "ocr_failed");
        assert_eq!(loaded.warnings[0].item_id.as_deref(), Some(image_item.id.as_str()));
    }

    #[test]
    fn ocr_item_missing_image_records_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (_manifest, mut h) = store.create(None, Producer::new("ctx")).unwrap();
        let image_item = h
            .add_image_item(
                ImageRole::Screenshot,
                "images/ghost.jpg",
                Some(Dimensions {
                    width: 5,
                    height: 5,
                }),
                ImageProvenance::default(),
                None,
            )
            .unwrap();
        let backend = OcrBackend::new("ocrs", vec![]);
        let out = ocr_item(&mut h, &image_item.id, &backend).unwrap();
        assert!(out.is_none());
        let loaded = store.load(h.id()).unwrap();
        assert_eq!(loaded.warnings[0].code, "ocr_failed");
    }

    #[test]
    fn capture_target_mode_mapping() {
        assert_eq!(CaptureTarget::Frontmost.mode(), CaptureMode::Frontmost);
        assert_eq!(CaptureTarget::Display.mode(), CaptureMode::Display);
        assert_eq!(CaptureTarget::All.mode(), CaptureMode::All);
    }
}
