use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    pub system: SystemInfo,
    pub displays: Vec<DisplayInfo>,
    pub windows: Vec<WindowInfo>,
    pub clipboard: ClipboardResult,
    pub screenshots: ScreenshotResult,
    pub accessibility: AccessibilityResult,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEnvelope {
    pub metadata: CaptureMetadata,
    pub context: CaptureResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureMetadata {
    pub session_id: String,
    pub captured_at: OffsetDateTime,
    pub version: u32,
}

impl CaptureEnvelope {
    pub fn new(context: CaptureResult) -> Self {
        Self {
            metadata: CaptureMetadata {
                session_id: Uuid::new_v4().to_string(),
                captured_at: OffsetDateTime::now_utc(),
                version: 1,
            },
            context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_has_metadata() {
        let context = CaptureResult {
            system: SystemInfo {
                platform: "test-os".to_string(),
                os_version: None,
                kernel_version: None,
                hostname: None,
            },
            displays: Vec::new(),
            windows: Vec::new(),
            clipboard: ClipboardResult {
                enabled: false,
                captured: false,
                note: None,
            },
            screenshots: ScreenshotResult {
                enabled: false,
                captures: Vec::new(),
            },
            accessibility: AccessibilityResult {
                enabled: false,
                captured: false,
                depth: 0,
                note: None,
            },
            notes: Vec::new(),
        };

        let envelope = CaptureEnvelope::new(context);
        assert!(!envelope.metadata.session_id.is_empty());

        let now = OffsetDateTime::now_utc();
        let delta = now - envelope.metadata.captured_at;
        assert!(delta.whole_seconds().abs() < 5, "timestamp should be recent");
        assert_eq!(envelope.metadata.version, 1);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub platform: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub index: u32,
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub scale_factor: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub app_name: Option<String>,
    pub title: Option<String>,
    pub focused: bool,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardResult {
    pub enabled: bool,
    pub captured: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub enabled: bool,
    pub captures: Vec<ScreenshotCapture>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotCapture {
    pub mode: ScreenshotMode,
    pub path: Option<PathBuf>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreenshotMode {
    Screen,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityResult {
    pub enabled: bool,
    pub captured: bool,
    pub depth: u8,
    pub note: Option<String>,
}
