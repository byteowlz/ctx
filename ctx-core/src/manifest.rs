//! Context bundle manifest (schema v1).
//!
//! A context bundle is the durable artifact shared between context collectors
//! (e.g. Omni Agent Handoff) and agent targets. `ctx` owns the format,
//! storage, and portable `.ctx` export.
//!
//! Layout: content [`Item`]s hold images/text/files/URLs/desktop snapshots,
//! while [`HandoffRecord`]s track dispatch metadata at the manifest level for
//! queue compatibility.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Current manifest schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Bundle lifecycle state.
///
/// Collection creates a bundle in the [`Draft`] state immediately; sending it
/// transitions to [`Sent`], and it may later be [`Archived`] or [`Cancelled`].
///
/// [`Draft`]: BundleState::Draft
/// [`Sent`]: BundleState::Sent
/// [`Archived`]: BundleState::Archived
/// [`Cancelled`]: BundleState::Cancelled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleState {
    /// Being collected; not yet dispatched.
    Draft,
    /// Dispatched to at least one target.
    Sent,
    /// Completed and retained for reference.
    Archived,
    /// Abandoned before sending.
    Cancelled,
}

/// Identifies the producer that created the bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Producer {
    /// Producer name, e.g. `omni` or `ctx`.
    pub name: String,
    /// Optional producer version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Producer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
        }
    }
}

/// The top-level bundle manifest stored at `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest schema version. See [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable bundle id, e.g. `2026-06-24T10-41-03Z_7hf3k2_build-slides`.
    pub id: String,
    /// When the bundle was first created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When the manifest was last updated.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Current lifecycle state.
    pub state: BundleState,
    /// Optional human-readable slug / title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Producer that created the bundle.
    pub producer: Producer,
    /// Content items: images, text, files, URLs, desktop snapshots.
    #[serde(default)]
    pub items: Vec<Item>,
    /// Handoff / dispatch records (queue-compatible).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handoffs: Vec<HandoffRecord>,
    /// Structured warnings (failed capture/OCR, missing permissions, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
    /// `.ctx` archive export metadata, set after an export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportMeta>,
    /// External references that could not be materialized on export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_refs: Vec<UnresolvedRef>,
}

impl Manifest {
    /// Create a new draft manifest with the given id and producer.
    pub fn new(id: impl Into<String>, producer: Producer) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            created_at: now,
            updated_at: now,
            state: BundleState::Draft,
            slug: None,
            producer,
            items: Vec::new(),
            handoffs: Vec::new(),
            warnings: Vec::new(),
            export: None,
            unresolved_refs: Vec::new(),
        }
    }

    /// Touch the `updated_at` timestamp to now (UTC).
    pub fn touch(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    /// Find an item by id.
    pub fn find_item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Find an item by id, mutably.
    pub fn find_item_mut(&mut self, id: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// Append a warning and refresh the updated timestamp.
    pub fn add_warning(&mut self, warning: Warning) {
        self.touch();
        self.warnings.push(warning);
    }
}

/// A content item with stable id, creation time, and a typed body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Stable item id unique within the bundle, e.g. `img_7hf3k2`.
    pub id: String,
    /// When the item was added.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Type-specific payload. The `kind` field is flattened into this object.
    #[serde(flatten)]
    pub body: ItemBody,
}

impl Item {
    pub fn new(id: impl Into<String>, body: ItemBody) -> Self {
        Self {
            id: id.into(),
            created_at: OffsetDateTime::now_utc(),
            body,
        }
    }

    /// Discriminant string for this item, e.g. `image`.
    pub fn kind(&self) -> &'static str {
        match &self.body {
            ItemBody::Image { .. } => "image",
            ItemBody::Text { .. } => "text",
            ItemBody::File { .. } => "file",
            ItemBody::Url { .. } => "url",
            ItemBody::DesktopSnapshot { .. } => "desktop_snapshot",
        }
    }
}

/// Typed item payload. Serialized with a `kind` discriminator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemBody {
    /// A raster image (screenshot, crop, or pasted image).
    Image(ImageFields),
    /// Text content (task, note, clipboard, OCR output, accessibility dump).
    Text(TextFields),
    /// A file (copied into the bundle or referenced by path).
    File(FileFields),
    /// A URL with best-effort provenance.
    Url(UrlFields),
    /// A structured desktop capture (ctx capture envelope).
    DesktopSnapshot(DesktopSnapshotFields),
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

/// How an image was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageRole {
    /// Full screenshot of a window or display.
    Screenshot,
    /// Cropped region of another image.
    Crop,
    /// Pasted or imported image.
    Pasted,
}

/// Screenshot capture mode provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    /// Frontmost / focused window.
    Frontmost,
    /// A single display.
    Display,
    /// All displays.
    All,
}

/// Pixel dimensions of an image.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// A crop rectangle in source pixels: x, y, width, height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Best-effort capture provenance for an image.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageProvenance {
    /// Source application name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Source window title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    /// URL provided by the producer, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Display index/name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Capture mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<CaptureMode>,
    /// Crop rect, if this image is a crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    /// When the capture happened.
    #[serde(with = "time::serde::rfc3339::option", default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFields {
    pub role: ImageRole,
    /// Path relative to the bundle directory.
    pub path: String,
    /// MIME type, e.g. `image/jpeg`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Pixel dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Dimensions>,
    /// Capture provenance.
    #[serde(default, skip_serializing_if = "is_default")]
    pub provenance: ImageProvenance,
    /// Ids of text items derived from OCR of this image.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ocr_refs: Vec<String>,
    /// Id of the source image this was cropped from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cropped_from: Option<String>,
    /// Free-form note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Purpose of a text item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextRole {
    /// The primary task / instruction for the agent.
    Task,
    /// A free-form note.
    Note,
    /// Clipboard contents.
    Clipboard,
    /// OCR output derived from an image.
    Ocr,
    /// Accessibility tree dump.
    Accessibility,
}

/// Where text content lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "storage", rename_all = "lowercase")]
pub enum TextContent {
    /// Inline text stored directly in the manifest.
    Inline { text: String },
    /// Text stored in a file relative to the bundle directory.
    File { path: String },
}

/// Source metadata for captured text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextFields {
    pub role: TextRole,
    #[serde(flatten)]
    pub content: TextContent,
    #[serde(default, skip_serializing_if = "is_default")]
    pub source: TextSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

/// File storage policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilePolicy {
    /// Bytes copied into the bundle.
    Copy,
    /// Path referenced in place (may need materialization on export).
    Reference,
    /// Let ctx decide based on heuristics.
    Auto,
}

/// A content-addressed file hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    /// Algorithm name, e.g. `sha256`.
    pub algorithm: String,
    /// Hex-encoded digest.
    pub value: String,
}

/// File storage details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "lowercase")]
pub enum FileStorage {
    /// Copied into the bundle under `bundle_path`.
    Copied {
        /// Path within the bundle directory.
        bundle_path: String,
        /// Original absolute path the file was copied from.
        original_path: String,
    },
    /// Referenced in place by absolute path.
    Referenced {
        /// Absolute path to the file (not copied).
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFields {
    #[serde(flatten)]
    pub storage: FileStorage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<FileHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl FileFields {
    /// The original/source path regardless of storage policy.
    pub fn source_path(&self) -> &str {
        match &self.storage {
            FileStorage::Copied { original_path, .. } => original_path,
            FileStorage::Referenced { path } => path,
        }
    }
}

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

/// Provenance for a captured URL.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UrlSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlFields {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub source: UrlSource,
}

// ---------------------------------------------------------------------------
// Desktop snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopSnapshotFields {
    /// Path relative to the bundle directory (a ctx capture envelope JSON).
    pub path: String,
    /// Format identifier, e.g. `ctx-capture-envelope`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Handoff records (dispatch metadata, queue-compatible)
// ---------------------------------------------------------------------------

/// How a bundle was dispatched to a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Local process spawn (e.g. `pi -p`, `claude -p`).
    LocalProcess,
    /// Existing tmux session.
    Tmux,
    /// Oqto / API endpoint.
    OqtoApi,
    /// Remote agent over the network.
    RemoteAgent,
    /// Queued for a long-lived worker.
    Queue,
}

/// Handoff lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffStatus {
    /// Waiting to be dispatched.
    Queued,
    /// Dispatched successfully.
    Sent,
    /// Confirmed delivered / accepted.
    Delivered,
    /// Dispatch failed.
    Failed,
}

/// Optional timestamps for a handoff record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HandoffTimestamps {
    #[serde(with = "time::serde::rfc3339::option", default, skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option", default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option", default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option", default, skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<OffsetDateTime>,
}

/// API/command metadata for a handoff target.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HandoffTargetMeta {
    /// Command/template, e.g. `pi -p {task}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// API endpoint or queue name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// A reference to an output or log artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Path relative to the bundle directory, or absolute.
    pub path: String,
}

/// A dispatch record linking a bundle to an agent target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffRecord {
    /// Target identifier, e.g. `pi-slides`, `oqto:sess_abc`.
    pub target_id: String,
    /// Transport used for dispatch.
    pub transport: Transport,
    /// Id of the bundle this record belongs to.
    pub bundle_id: String,
    /// Current status.
    pub status: HandoffStatus,
    /// Timestamps for each lifecycle transition.
    #[serde(default, skip_serializing_if = "is_default")]
    pub timestamps: HandoffTimestamps,
    /// Stable key for session affinity (send more tasks to the same agent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_affinity_key: Option<String>,
    /// Command/API metadata.
    #[serde(default, skip_serializing_if = "is_default")]
    pub target_meta: HandoffTargetMeta,
    /// Path to agent output artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ArtifactRef>,
    /// Path to dispatch log artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<ArtifactRef>,
    /// Free-form note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Warnings, export metadata, unresolved references
// ---------------------------------------------------------------------------

/// A structured, non-fatal problem recorded on the bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    /// Stable code, e.g. `ocr_failed`, `permission_denied`.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Related item id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    /// When the warning was recorded.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl Warning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            item_id: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn for_item(mut self, item_id: impl Into<String>) -> Self {
        self.item_id = Some(item_id.into());
        self
    }
}

/// `.ctx` portable archive export metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMeta {
    /// Archive format identifier, always `ctx-archive`.
    pub format: String,
    /// Archive layout version.
    pub archive_version: u32,
    /// When the export was produced.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Path to the `.ctx` archive relative to the bundle directory.
    pub archive_path: String,
    /// Path to the manifest inside the archive (`manifest.json`).
    pub manifest_path: String,
    /// Path to the agent-readable context inside the archive (`context.md`).
    pub context_md_path: String,
}

/// An external file reference that could not be materialized on export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedRef {
    /// Related item id.
    pub item_id: String,
    /// The path that could not be resolved.
    pub path: String,
    /// Why it could not be resolved.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// serde helper
// ---------------------------------------------------------------------------

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_with_all_item_kinds() {
        let mut manifest = Manifest::new(
            "2026-06-24T10-41-03Z_7hf3k2_build-slides",
            Producer {
                name: "omni".to_string(),
                version: Some("0.1.0".to_string()),
            },
        );
        manifest.slug = Some("build-slides".to_string());

        manifest.items.push(Item::new(
            "img_1",
            ItemBody::Image(ImageFields {
                role: ImageRole::Screenshot,
                path: "images/screenshot-1.jpg".to_string(),
                mime: Some("image/jpeg".to_string()),
                dimensions: Some(Dimensions {
                    width: 1920,
                    height: 1080,
                }),
                provenance: ImageProvenance {
                    app: Some("Keynote".to_string()),
                    window_title: Some("Slides.key".to_string()),
                    display: Some("0".to_string()),
                    mode: Some(CaptureMode::Frontmost),
                    ..Default::default()
                },
                ocr_refs: Vec::new(),
                cropped_from: None,
                note: None,
            }),
        ));

        manifest.items.push(Item::new(
            "txt_1",
            ItemBody::Text(TextFields {
                role: TextRole::Task,
                content: TextContent::Inline {
                    text: "Build a 5-slide deck about context bundles".to_string(),
                },
                source: TextSource::default(),
                note: None,
            }),
        ));

        manifest.items.push(Item::new(
            "file_1",
            ItemBody::File(FileFields {
                storage: FileStorage::Copied {
                    bundle_path: "files/deck.json".to_string(),
                    original_path: "/Users/me/deck.json".to_string(),
                },
                size_bytes: Some(2048),
                mime: Some("application/json".to_string()),
                hash: Some(FileHash {
                    algorithm: "sha256".to_string(),
                    value: "abc123".to_string(),
                }),
                note: None,
            }),
        ));

        manifest.items.push(Item::new(
            "url_1",
            ItemBody::Url(UrlFields {
                url: "https://example.com/spec".to_string(),
                title: Some("Spec".to_string()),
                source: UrlSource {
                    app: Some("Safari".to_string()),
                    window_title: None,
                },
            }),
        ));

        manifest.items.push(Item::new(
            "snap_1",
            ItemBody::DesktopSnapshot(DesktopSnapshotFields {
                path: "snapshots/desktop.json".to_string(),
                format: Some("ctx-capture-envelope".to_string()),
                note: None,
            }),
        ));

        manifest.handoffs.push(HandoffRecord {
            target_id: "pi-slides".to_string(),
            transport: Transport::LocalProcess,
            bundle_id: manifest.id.clone(),
            status: HandoffStatus::Sent,
            timestamps: HandoffTimestamps {
                sent_at: Some(OffsetDateTime::now_utc()),
                ..Default::default()
            },
            session_affinity_key: Some("slides".to_string()),
            target_meta: HandoffTargetMeta {
                command: Some("pi -p {task}".to_string()),
                ..Default::default()
            },
            output: None,
            log: None,
            note: None,
        });

        let json = serde_json::to_string_pretty(&manifest).expect("serialize");
        let back: Manifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.schema_version, SCHEMA_VERSION);
        assert_eq!(back.id, "2026-06-24T10-41-03Z_7hf3k2_build-slides");
        assert_eq!(back.state, BundleState::Draft);
        assert_eq!(back.items.len(), 5);
        assert_eq!(back.items[0].kind(), "image");
        assert_eq!(back.items[1].kind(), "text");
        assert_eq!(back.items[2].kind(), "file");
        assert_eq!(back.items[3].kind(), "url");
        assert_eq!(back.items[4].kind(), "desktop_snapshot");
        assert_eq!(back.handoffs.len(), 1);
        assert_eq!(back.handoffs[0].status, HandoffStatus::Sent);
    }

    #[test]
    fn item_kind_discriminant_serializes() {
        let item = Item::new(
            "txt_task",
            ItemBody::Text(TextFields {
                role: TextRole::Task,
                content: TextContent::Inline {
                    text: "do thing".to_string(),
                },
                source: TextSource::default(),
                note: None,
            }),
        );
        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(value["kind"], "text");
        assert_eq!(value["role"], "task");
        assert_eq!(value["storage"], "inline");
        assert_eq!(value["text"], "do thing");
    }

    #[test]
    fn file_storage_tag_roundtrips() {
        let fields = FileFields {
            storage: FileStorage::Referenced {
                path: "/abs/path.txt".to_string(),
            },
            size_bytes: None,
            mime: None,
            hash: None,
            note: None,
        };
        let value = serde_json::to_value(&fields).expect("serialize");
        assert_eq!(value["policy"], "referenced");
        assert_eq!(value["path"], "/abs/path.txt");
        let back: FileFields = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back.source_path(), "/abs/path.txt");
    }

    #[test]
    fn warning_for_item_chains() {
        let w = Warning::new("ocr_failed", "ocrs not found").for_item("img_1");
        assert_eq!(w.code, "ocr_failed");
        assert_eq!(w.item_id.as_deref(), Some("img_1"));
    }

    /// Validate the bundled example manifests deserialize into the typed
    /// [`Manifest`], proving the Rust schema and JSON schema agree.
    fn load_example(rel: &str) -> Manifest {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(&manifest_dir)
            .join("..")
            .join("examples")
            .join("bundles")
            .join(rel);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read example {rel} at {}: {err}", path.display()));
        serde_json::from_str(&content)
            .unwrap_or_else(|err| panic!("parse example {rel}: {err}"))
    }

    #[test]
    fn example_slide_handoff_validates() {
        let manifest = load_example("slide-handoff.json");
        assert_eq!(manifest.schema_version, SCHEMA_VERSION);
        assert_eq!(manifest.state, BundleState::Sent);
        assert_eq!(manifest.slug.as_deref(), Some("build-slides"));
        // task, screenshot, crop, ocr text, file, url
        assert_eq!(manifest.items.len(), 6);
        assert_eq!(manifest.handoffs.len(), 1);
        // image references its OCR text item
        let img = manifest.find_item("img_1").expect("img_1 exists");
        let ImageFields { ocr_refs, .. } = match &img.body {
            ItemBody::Image(f) => f,
            _ => panic!("expected image"),
        };
        assert!(ocr_refs.iter().any(|r| r == "txt_ocr1"));
    }

    #[test]
    fn example_generic_pi_handoff_validates() {
        let manifest = load_example("generic-pi-handoff.json");
        assert_eq!(manifest.state, BundleState::Draft);
        assert_eq!(manifest.items.len(), 4);
        // includes a referenced file and a desktop snapshot
        assert_eq!(manifest.items[3].kind(), "file");
        assert_eq!(manifest.items[1].kind(), "desktop_snapshot");
        assert_eq!(manifest.warnings.len(), 1);
        assert_eq!(manifest.warnings[0].code, "permission_denied");
        assert!(manifest.handoffs.is_empty());
    }
}


