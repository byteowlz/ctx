use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use arboard::Clipboard;
use image::{ImageBuffer, ImageError, Rgb, Rgba};
use screenshots::Screen;
use sysinfo::System;
use uuid::Uuid;
use x_win::{get_active_window, get_open_windows};

use crate::capture::{
    AccessibilityResult, ActionSupport, AppInfo, CaptureResult, ClipboardResult, DisplayInfo,
    ScreenshotCapture, ScreenshotMode, ScreenshotResult, SystemInfo, WindowInfo,
};
use crate::config::CaptureConfig;

#[cfg(target_os = "macos")]
use accessibility::{AXUIElement, AXUIElementAttributes};

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub capture_dir: PathBuf,
    pub include_clipboard: bool,
    pub include_screenshots: bool,
    pub include_accessibility: bool,
    pub include_actions: bool,
    pub accessibility_depth: u8,
    pub image_quality: u8,
    pub screenshot_timeout_ms: u64,
}

impl CaptureRequest {
    pub fn from_config(cfg: &CaptureConfig, capture_dir: PathBuf) -> Self {
        Self {
            capture_dir,
            include_clipboard: cfg.include_clipboard,
            include_screenshots: cfg.include_screenshots,
            include_accessibility: cfg.include_accessibility,
            include_actions: cfg.include_actions,
            accessibility_depth: cfg.accessibility_depth,
            image_quality: cfg.image_quality,
            screenshot_timeout_ms: cfg.screenshot_timeout_ms,
        }
    }
}

pub trait ContextProvider {
    fn capture(&self, request: &CaptureRequest) -> anyhow::Result<CaptureResult>;
}

#[derive(Debug, Default)]
pub struct NoopPlatform;

impl ContextProvider for NoopPlatform {
    fn capture(&self, request: &CaptureRequest) -> anyhow::Result<CaptureResult> {
        let system = SystemInfo {
            platform: env::consts::OS.to_string(),
            os_version: System::long_os_version().or_else(System::os_version),
            kernel_version: System::kernel_version(),
            hostname: System::host_name().or_else(|| env::var("HOSTNAME").ok()),
        };

        let clipboard = ClipboardResult {
            enabled: request.include_clipboard,
            captured: false,
            note: if request.include_clipboard {
                Some(
                    "Clipboard capture is not implemented yet; enable platform provider to collect data."
                        .to_string(),
                )
            } else {
                Some("Clipboard capture disabled in configuration.".to_string())
            },
        };

        let screenshots = ScreenshotResult {
            enabled: request.include_screenshots,
            captures: if request.include_screenshots {
                vec![ScreenshotCapture {
                    mode: ScreenshotMode::Screen,
                    path: None,
                    note: Some(
                        "Screenshot capture is not implemented yet; platform-specific implementation required."
                            .to_string(),
                    ),
                }]
            } else {
                Vec::new()
            },
        };

        let accessibility = AccessibilityResult {
            enabled: request.include_accessibility,
            captured: false,
            depth: request.accessibility_depth,
            note: if request.include_accessibility {
                Some(
                    "Accessibility capture is not implemented yet; platform-specific implementation required."
                        .to_string(),
                )
            } else {
                Some("Accessibility capture disabled in configuration.".to_string())
            },
            focused: None,
            path: Vec::new(),
        };

        let actions = ActionSupport {
            enabled: request.include_actions,
            supported: false,
            note: if request.include_actions {
                Some(
                    "Action layer is not implemented yet; platform-specific implementation required."
                        .to_string(),
                )
            } else {
                Some("Action layer disabled in configuration.".to_string())
            },
        };

        let mut notes = Vec::new();
        if !request.include_clipboard {
            notes.push("Clipboard capture disabled in configuration.".to_string());
        }
        if !request.include_screenshots {
            notes.push("Screenshot capture disabled in configuration.".to_string());
        }
        if !request.include_accessibility {
            notes.push("Accessibility capture disabled in configuration.".to_string());
        }
        if !request.include_actions {
            notes.push("Action layer disabled in configuration.".to_string());
        }
        if notes.is_empty() {
            notes.push(
                "Platform capture is stubbed; hook up platform-specific providers.".to_string(),
            );
        }

        Ok(CaptureResult {
            system,
            displays: Vec::new(),
            apps: Vec::new(),
            windows: Vec::new(),
            clipboard,
            screenshots,
            accessibility,
            actions,
            notes,
        })
    }
}

#[derive(Debug, Default)]
pub struct DesktopPlatform;

impl ContextProvider for DesktopPlatform {
    fn capture(&self, request: &CaptureRequest) -> anyhow::Result<CaptureResult> {
        let mut notes = Vec::new();

        let system = SystemInfo {
            platform: env::consts::OS.to_string(),
            os_version: System::long_os_version().or_else(System::os_version),
            kernel_version: System::kernel_version(),
            hostname: System::host_name().or_else(|| env::var("HOSTNAME").ok()),
        };

        let displays = match Screen::all() {
            Ok(screens) => screens
                .into_iter()
                .enumerate()
                .map(|(index, screen)| DisplayInfo {
                    index: index as u32,
                    name: None,
                    width: screen.display_info.width,
                    height: screen.display_info.height,
                    scale_factor: Some(screen.display_info.scale_factor),
                })
                .collect(),
            Err(err) => {
                notes.push(format!("Failed to enumerate displays: {err}"));
                Vec::new()
            }
        };

        let (windows, apps) = capture_windows_and_apps(&mut notes);

        let clipboard = if request.include_clipboard {
            match Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(text) => ClipboardResult {
                    enabled: true,
                    captured: true,
                    note: Some(format!("Captured text ({} chars)", text.len())),
                },
                Err(err) => {
                    notes.push(format!("Clipboard capture failed: {err}"));
                    ClipboardResult {
                        enabled: true,
                        captured: false,
                        note: Some("Clipboard unavailable".to_string()),
                    }
                }
            }
        } else {
            ClipboardResult {
                enabled: false,
                captured: false,
                note: Some("Clipboard capture disabled in configuration.".to_string()),
            }
        };

        let screenshots = if request.include_screenshots {
            match capture_screens(&request.capture_dir, request.image_quality) {
                Ok(paths) => ScreenshotResult {
                    enabled: true,
                    captures: paths
                        .into_iter()
                        .map(|path| ScreenshotCapture {
                            mode: ScreenshotMode::Screen,
                            path: Some(path),
                            note: Some("Display capture completed.".to_string()),
                        })
                        .collect(),
                },
                Err(err) => {
                    notes.push(format!("Screenshot capture failed: {err}"));
                    ScreenshotResult {
                        enabled: true,
                        captures: vec![ScreenshotCapture {
                            mode: ScreenshotMode::Screen,
                            path: None,
                            note: Some("Screenshot failed; see notes.".to_string()),
                        }],
                    }
                }
            }
        } else {
            ScreenshotResult {
                enabled: false,
                captures: Vec::new(),
            }
        };

        let accessibility = capture_accessibility(request, &mut notes);

        let actions = ActionSupport {
            enabled: request.include_actions,
            supported: false,
            note: if request.include_actions {
                Some(
                    "Action layer is not implemented yet; platform-specific implementation required."
                        .to_string(),
                )
            } else {
                Some("Action layer disabled in configuration.".to_string())
            },
        };

        if !request.include_accessibility {
            notes.push("Accessibility capture disabled in configuration.".to_string());
        }
        if !request.include_actions {
            notes.push("Action layer disabled in configuration.".to_string());
        }

        Ok(CaptureResult {
            system,
            displays,
            apps,
            windows,
            clipboard,
            screenshots,
            accessibility,
            actions,
            notes,
        })
    }
}

fn capture_screens(capture_dir: &PathBuf, quality: u8) -> Result<Vec<PathBuf>, CaptureError> {
    std::fs::create_dir_all(capture_dir)?;

    let screens = Screen::all().map_err(CaptureError::Screenshots)?;
    if screens.is_empty() {
        return Err(CaptureError::Custom("No displays found".to_string()));
    }

    let mut paths = Vec::with_capacity(screens.len());
    for screen in screens {
        let image = screen.capture().map_err(CaptureError::Screenshots)?;
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_vec(image.width(), image.height(), image.to_vec()).ok_or_else(
                || CaptureError::Custom("Failed to read screenshot buffer".to_string()),
            )?;

        let file_name = format!("screenshot-{}.jpg", Uuid::new_v4());
        let path = capture_dir.join(file_name);
        save_jpeg(&buffer, &path, quality)?;
        paths.push(path);
    }

    Ok(paths)
}

fn save_jpeg(
    buffer: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    path: &PathBuf,
    quality: u8,
) -> Result<(), CaptureError> {
    // Convert RGBA to RGB (JPEG doesn't support alpha channel)
    let rgb_buffer: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(buffer.width(), buffer.height(), |x, y| {
            let pixel = buffer.get_pixel(x, y);
            Rgb([pixel[0], pixel[1], pixel[2]])
        });

    let mut file = std::fs::File::create(path)?;
    let quality = quality.clamp(1, 100);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality);
    encoder
        .encode(
            rgb_buffer.as_raw(),
            rgb_buffer.width(),
            rgb_buffer.height(),
            image::ColorType::Rgb8.into(),
        )
        .map_err(CaptureError::ImageSave)
}

fn capture_windows_and_apps(notes: &mut Vec<String>) -> (Vec<WindowInfo>, Vec<AppInfo>) {
    let active = get_active_window().ok();
    let windows = match get_open_windows() {
        Ok(list) => list,
        Err(err) => {
            notes.push(format!("Window enumeration failed: {err}"));
            return (Vec::new(), Vec::new());
        }
    };

    let mut apps_by_pid: HashMap<u32, AppInfo> = HashMap::new();
    let mut mapped = Vec::with_capacity(windows.len());
    for window in windows {
        let focused = active.as_ref().map(|a| a.id == window.id).unwrap_or(false);
        let pid = window.info.process_id;
        let app_entry = apps_by_pid.entry(pid).or_insert_with(|| AppInfo {
            name: Some(window.info.name.clone()),
            bundle_id: None,
            pid: Some(pid),
            focused: false,
        });
        if focused {
            app_entry.focused = true;
        }

        mapped.push(WindowInfo {
            id: Some(window.id as u64),
            app_name: Some(window.info.name.clone()),
            app_bundle_id: None,
            pid: Some(pid),
            title: Some(window.title.clone()),
            focused,
            visible: true,
            bounds: Some(crate::capture::Bounds {
                x: window.position.x,
                y: window.position.y,
                width: window.position.width as u32,
                height: window.position.height as u32,
            }),
        });
    }

    let apps = apps_by_pid.into_values().collect::<Vec<_>>();
    (mapped, apps)
}

fn capture_accessibility(request: &CaptureRequest, notes: &mut Vec<String>) -> AccessibilityResult {
    if !request.include_accessibility {
        return AccessibilityResult {
            enabled: false,
            captured: false,
            depth: request.accessibility_depth,
            note: Some("Accessibility capture disabled in configuration.".to_string()),
            focused: None,
            path: Vec::new(),
        };
    }

    #[cfg(target_os = "macos")]
    {
        // Get the active window's PID to create an application element
        let active_window = match get_active_window() {
            Ok(w) => w,
            Err(err) => {
                notes.push(format!(
                    "Accessibility capture failed: could not get active window: {err}"
                ));
                return AccessibilityResult {
                    enabled: true,
                    captured: false,
                    depth: request.accessibility_depth,
                    note: Some("No active window found; see notes.".to_string()),
                    focused: None,
                    path: Vec::new(),
                };
            }
        };

        let pid = active_window.info.process_id as i32;
        let app_element = AXUIElement::application(pid);

        // Get the focused window from the application
        let focused = match app_element.focused_window() {
            Ok(element) => element,
            Err(err) => {
                notes.push(format!("Accessibility capture failed: {err}"));
                return AccessibilityResult {
                    enabled: true,
                    captured: false,
                    depth: request.accessibility_depth,
                    note: Some("Accessibility unavailable; see notes.".to_string()),
                    focused: None,
                    path: Vec::new(),
                };
            }
        };

        let focused_node = snapshot_node(&focused);
        let mut path = Vec::new();
        let mut current = focused;
        for _ in 0..=request.accessibility_depth {
            path.push(snapshot_node(&current));
            match current.parent() {
                Ok(parent) => current = parent,
                Err(_) => break,
            }
        }
        path.reverse();

        return AccessibilityResult {
            enabled: true,
            captured: true,
            depth: request.accessibility_depth,
            note: Some("Captured focused window snapshot.".to_string()),
            focused: Some(focused_node),
            path,
        };
    }

    #[cfg(not(target_os = "macos"))]
    {
        notes.push("Accessibility capture is not supported on this platform.".to_string());
        AccessibilityResult {
            enabled: true,
            captured: false,
            depth: request.accessibility_depth,
            note: Some("Accessibility unsupported on this platform.".to_string()),
            focused: None,
            path: Vec::new(),
        }
    }
}

#[cfg(target_os = "macos")]
fn snapshot_node(element: &AXUIElement) -> crate::capture::AccessibilityNode {
    let role = element.role().ok().map(|value| value.to_string());
    let label = element
        .description()
        .ok()
        .or_else(|| element.title().ok())
        .map(|value| value.to_string());
    let value = element.value().ok().map(|value| format!("{value:?}"));
    let enabled = element.enabled().ok().map(bool::from);

    crate::capture::AccessibilityNode {
        role,
        label,
        value,
        enabled,
        frame: None,
    }
}

#[derive(thiserror::Error, Debug)]
enum CaptureError {
    #[error("screenshot error: {0}")]
    Screenshots(#[source] anyhow::Error),
    #[error("image save error: {0}")]
    ImageSave(#[source] ImageError),
    #[error("{0}")]
    Custom(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
