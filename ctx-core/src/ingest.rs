//! Screenshot source detection: watch screenshot directories and the
//! clipboard for new screenshots, deduplicating across sources.
//!
//! Detection is poll-based rather than inotify-based so it behaves the same
//! on every platform (including Wayland) without extra dependencies. Dedup
//! hashes decoded RGBA pixels plus dimensions, so the same screenshot
//! arriving both as a clipboard image and as an encoded file collides.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::store::sha256_chunks;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];
const SEEN_INDEX_CAP: usize = 2048;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to decode image {path}: {source}")]
    Decode {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("failed to encode clipboard image: {0}")]
    Encode(image::ImageError),
    #[error("invalid clipboard pixel buffer ({width}x{height}, {len} bytes)")]
    InvalidPixels { width: u32, height: u32, len: usize },
    #[error("failed to serialize seen index: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenshotSource {
    Clipboard,
    File,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotEvent {
    pub source: ScreenshotSource,
    pub path: PathBuf,
    /// SHA-256 hex over width, height, and decoded RGBA pixel bytes.
    pub hash: String,
    pub width: u32,
    pub height: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub detected_at: OffsetDateTime,
}

pub fn pixel_hash(rgba: &[u8], width: u32, height: u32) -> String {
    sha256_chunks([
        width.to_le_bytes().as_slice(),
        height.to_le_bytes().as_slice(),
        rgba,
    ])
    .value
}

/// Platform-typical screenshot target directories that exist on this machine.
pub fn default_watch_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(pictures) = dirs::picture_dir() {
        dirs.push(pictures.join("Screenshots"));
    }
    if cfg!(target_os = "macos")
        && let Some(desktop) = dirs::desktop_dir()
    {
        dirs.push(desktop);
    }
    dirs.retain(|dir| dir.is_dir());
    dirs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileSig {
    len: u64,
    mtime: Option<SystemTime>,
}

/// Poll-based directory scanner. A file is reported once its size and mtime
/// are stable across two consecutive scans (writers may still be flushing),
/// and only if it was modified recently (`max_age`).
#[derive(Debug)]
pub struct DirScanner {
    dirs: Vec<PathBuf>,
    max_age: Option<Duration>,
    known: HashSet<PathBuf>,
    pending: HashMap<PathBuf, FileSig>,
}

impl DirScanner {
    #[must_use]
    pub fn new(dirs: Vec<PathBuf>, max_age: Option<Duration>) -> Self {
        Self {
            dirs,
            max_age,
            known: HashSet::new(),
            pending: HashMap::new(),
        }
    }

    pub fn scan(&mut self) -> Vec<PathBuf> {
        let now = SystemTime::now();
        let mut ready = Vec::new();
        self.pending.retain(|path, _| path.exists());
        for dir in &self.dirs {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if self.known.contains(&path) || !has_image_extension(&path) {
                    continue;
                }
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let sig = FileSig {
                    len: meta.len(),
                    mtime: meta.modified().ok(),
                };
                if let (Some(max_age), Some(mtime)) = (self.max_age, sig.mtime)
                    && now.duration_since(mtime).unwrap_or(Duration::ZERO) > max_age
                {
                    continue;
                }
                if self.pending.get(&path) == Some(&sig) {
                    self.pending.remove(&path);
                    self.known.insert(path.clone());
                    ready.push(path);
                } else {
                    self.pending.insert(path, sig);
                }
            }
        }
        ready
    }
}

fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Persistent set of already-ingested content hashes, so restarts and
/// clipboard/file duplicates of the same screenshot emit only one event.
#[derive(Debug)]
pub struct SeenIndex {
    path: PathBuf,
    hashes: HashMap<String, String>,
}

impl SeenIndex {
    pub fn load(path: PathBuf) -> Self {
        let hashes = fs::read_to_string(&path)
            .ok()
            .and_then(|payload| serde_json::from_str(&payload).ok())
            .unwrap_or_default();
        Self { path, hashes }
    }

    /// Record a hash; returns true when it was not seen before.
    pub fn mark_new(&mut self, hash: &str) -> bool {
        if self.hashes.contains_key(hash) {
            return false;
        }
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        self.hashes.insert(hash.to_string(), now);
        self.prune();
        true
    }

    fn prune(&mut self) {
        while self.hashes.len() > SEEN_INDEX_CAP {
            // Rfc3339 UTC timestamps sort chronologically as strings.
            let Some(oldest) = self
                .hashes
                .iter()
                .min_by(|a, b| a.1.cmp(b.1))
                .map(|(hash, _)| hash.clone())
            else {
                break;
            };
            self.hashes.remove(&oldest);
        }
    }

    pub fn save(&self) -> Result<(), IngestError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| IngestError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let payload = serde_json::to_vec_pretty(&self.hashes)?;
        fs::write(&self.path, payload).map_err(|source| IngestError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DetectorOptions {
    pub watch_dirs: Vec<PathBuf>,
    pub seen_index_path: PathBuf,
    /// Directory where clipboard-only screenshots are spooled as PNG files.
    pub inbox_dir: PathBuf,
    pub max_age: Option<Duration>,
}

#[derive(Debug)]
pub struct ScreenshotDetector {
    scanner: DirScanner,
    seen: SeenIndex,
    inbox_dir: PathBuf,
}

impl ScreenshotDetector {
    #[must_use]
    pub fn new(options: DetectorOptions) -> Self {
        Self {
            scanner: DirScanner::new(options.watch_dirs, options.max_age),
            seen: SeenIndex::load(options.seen_index_path),
            inbox_dir: options.inbox_dir,
        }
    }

    /// Scan watch directories and return events for newly detected, not yet
    /// seen screenshots. Files that fail to decode are skipped.
    pub fn poll_files(&mut self) -> Vec<ScreenshotEvent> {
        let mut events = Vec::new();
        for path in self.scanner.scan() {
            let Ok(image) = image::open(&path) else {
                continue;
            };
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            let hash = pixel_hash(rgba.as_raw(), width, height);
            if self.seen.mark_new(&hash) {
                events.push(ScreenshotEvent {
                    source: ScreenshotSource::File,
                    path,
                    hash,
                    width,
                    height,
                    detected_at: OffsetDateTime::now_utc(),
                });
            }
        }
        if !events.is_empty() {
            let _ = self.seen.save();
        }
        events
    }

    /// Ingest a raw RGBA pixel buffer (e.g. a clipboard image). New content is
    /// spooled as a PNG into the inbox directory; duplicates return None.
    pub fn ingest_pixels(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Option<ScreenshotEvent>, IngestError> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(IngestError::InvalidPixels {
                width,
                height,
                len: rgba.len(),
            });
        }
        let hash = pixel_hash(rgba, width, height);
        if !self.seen.mark_new(&hash) {
            return Ok(None);
        }
        fs::create_dir_all(&self.inbox_dir).map_err(|source| IngestError::Io {
            path: self.inbox_dir.clone(),
            source,
        })?;
        let path = self
            .inbox_dir
            .join(format!("clipboard-{}.png", &hash[..16]));
        let buffer = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or(
            IngestError::InvalidPixels {
                width,
                height,
                len: rgba.len(),
            },
        )?;
        buffer
            .save_with_format(&path, image::ImageFormat::Png)
            .map_err(IngestError::Encode)?;
        let _ = self.seen.save();
        Ok(Some(ScreenshotEvent {
            source: ScreenshotSource::Clipboard,
            path,
            hash,
            width,
            height,
            detected_at: OffsetDateTime::now_utc(),
        }))
    }
}

/// Read the current clipboard image as RGBA bytes, if the clipboard holds one.
pub fn read_clipboard_image() -> Option<(Vec<u8>, u32, u32)> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let img = clipboard.get_image().ok()?;
    let width = u32::try_from(img.width).ok()?;
    let height = u32::try_from(img.height).ok()?;
    Some((img.bytes.into_owned(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, seed: u8, width: u32, height: u32) {
        let img = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([seed, x as u8, y as u8, 255])
        });
        img.save_with_format(path, image::ImageFormat::Png)
            .expect("write png");
    }

    fn detector(root: &Path) -> ScreenshotDetector {
        ScreenshotDetector::new(DetectorOptions {
            watch_dirs: vec![root.join("shots")],
            seen_index_path: root.join("state").join("seen.json"),
            inbox_dir: root.join("inbox"),
            max_age: None,
        })
    }

    #[test]
    fn pixel_hash_depends_on_dimensions_and_content() {
        let pixels = vec![7u8; 16];
        assert_eq!(pixel_hash(&pixels, 2, 2), pixel_hash(&pixels, 2, 2));
        assert_ne!(pixel_hash(&pixels, 2, 2), pixel_hash(&pixels, 4, 1));
        let other = vec![8u8; 16];
        assert_ne!(pixel_hash(&pixels, 2, 2), pixel_hash(&other, 2, 2));
    }

    #[test]
    fn detects_new_file_once_after_settling() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("shots")).unwrap();
        let mut det = detector(temp.path());

        write_png(&temp.path().join("shots/shot.png"), 1, 4, 4);
        assert!(det.poll_files().is_empty(), "first scan only records sig");

        let events = det.poll_files();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, ScreenshotSource::File);
        assert_eq!((events[0].width, events[0].height), (4, 4));

        assert!(det.poll_files().is_empty(), "no re-emit for same file");
    }

    #[test]
    fn ignores_non_image_and_stale_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let shots = temp.path().join("shots");
        fs::create_dir_all(&shots).unwrap();
        fs::write(shots.join("notes.txt"), b"not an image").unwrap();
        write_png(&shots.join("old.png"), 2, 4, 4);

        let mut det = ScreenshotDetector::new(DetectorOptions {
            watch_dirs: vec![shots],
            seen_index_path: temp.path().join("seen.json"),
            inbox_dir: temp.path().join("inbox"),
            max_age: Some(Duration::ZERO),
        });
        assert!(det.poll_files().is_empty());
        assert!(det.poll_files().is_empty());
    }

    #[test]
    fn clipboard_duplicate_of_file_is_deduplicated() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("shots")).unwrap();
        let mut det = detector(temp.path());

        let path = temp.path().join("shots/shot.png");
        write_png(&path, 3, 4, 4);
        det.poll_files();
        let events = det.poll_files();
        assert_eq!(events.len(), 1);

        let rgba = image::open(&path).unwrap().to_rgba8();
        let duplicate = det.ingest_pixels(rgba.as_raw(), 4, 4).expect("ingest");
        assert!(duplicate.is_none(), "same pixels via clipboard are deduped");

        let fresh = det.ingest_pixels(&[9u8; 64], 4, 4).expect("ingest");
        let event = fresh.expect("new clipboard content emits event");
        assert_eq!(event.source, ScreenshotSource::Clipboard);
        assert!(event.path.exists(), "clipboard image spooled to inbox");
    }

    #[test]
    fn seen_index_persists_across_restarts() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("shots")).unwrap();
        write_png(&temp.path().join("shots/shot.png"), 4, 4, 4);

        let mut first = detector(temp.path());
        first.poll_files();
        assert_eq!(first.poll_files().len(), 1);

        let mut second = detector(temp.path());
        second.poll_files();
        assert!(
            second.poll_files().is_empty(),
            "hash persisted, file not re-emitted after restart"
        );
    }

    #[test]
    fn ingest_pixels_rejects_mismatched_buffer() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut det = detector(temp.path());
        let result = det.ingest_pixels(&[0u8; 10], 4, 4);
        assert!(matches!(result, Err(IngestError::InvalidPixels { .. })));
    }
}
