use std::env;
use std::path::PathBuf;

use arboard::Clipboard;
use image::{ImageBuffer, ImageError, Rgba};
use screenshots::Screen;
use sysinfo::System;
use uuid::Uuid;

use crate::capture::{
    AccessibilityResult, CaptureResult, ClipboardResult, DisplayInfo, ScreenshotCapture,
    ScreenshotMode, ScreenshotResult, SystemInfo, WindowInfo,
};
use crate::config::CaptureConfig;

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub capture_dir: PathBuf,
    pub include_clipboard: bool,
    pub include_screenshots: bool,
    pub include_accessibility: bool,
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
        if notes.is_empty() {
            notes.push("Platform capture is stubbed; hook up platform-specific providers.".to_string());
        }

        Ok(CaptureResult {
            system,
            displays: Vec::new(),
            windows: Vec::new(),
            clipboard,
            screenshots,
            accessibility,
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
                    scale_factor: Some(screen.display_info.scale_factor as f32),
                })
                .collect(),
            Err(err) => {
                notes.push(format!("Failed to enumerate displays: {err}"));
                Vec::new()
            }
        };

        let windows: Vec<WindowInfo> = Vec::new();

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
            match capture_primary_screen(&request.capture_dir) {
                Ok(path) => ScreenshotResult {
                    enabled: true,
                    captures: vec![ScreenshotCapture {
                        mode: ScreenshotMode::Screen,
                        path: Some(path),
                        note: Some("Primary display capture completed.".to_string()),
                    }],
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

        let accessibility = AccessibilityResult {
            enabled: request.include_accessibility,
            captured: false,
            depth: request.accessibility_depth,
            note: Some(
                "Accessibility capture not implemented yet; platform-specific bridge required."
                    .to_string(),
            ),
        };

        if !request.include_accessibility {
            notes.push("Accessibility capture disabled in configuration.".to_string());
        }

        Ok(CaptureResult {
            system,
            displays,
            windows,
            clipboard,
            screenshots,
            accessibility,
            notes,
        })
    }
}

fn capture_primary_screen(capture_dir: &PathBuf) -> Result<PathBuf, CaptureError> {
    std::fs::create_dir_all(capture_dir)?;

    let screen = Screen::all()
        .map_err(CaptureError::Screenshots)?
        .into_iter()
        .next()
        .ok_or_else(|| CaptureError::Custom("No displays found".to_string()))?;

    let image = screen.capture().map_err(CaptureError::Screenshots)?;
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_vec(
        image.width() as u32,
        image.height() as u32,
        image.to_vec(),
    )
    .ok_or_else(|| CaptureError::Custom("Failed to read screenshot buffer".to_string()))?;

    let file_name = format!("screenshot-{}.png", Uuid::new_v4());
    let path = capture_dir.join(file_name);
    buffer.save(&path).map_err(CaptureError::ImageSave)?;

    Ok(path)
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
