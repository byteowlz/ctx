//! Context bundle store: central, app-independent storage for context bundles.
//!
//! A bundle is a directory under the configured `bundle_dir`, identified by a
//! stable id of the form `<timestamp>_<shortid>_<slug?>`. The store owns:
//!
//! - deterministic id generation ([`generate_id`]),
//! - creating/loading/saving the [`Manifest`] (`manifest.json`),
//! - rendering agent-readable `context.md`,
//! - materializing item payloads (images, files, text) under the bundle.
//!
//! See the wiki design note `projects/agent-handoff-context-bundles.md`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::manifest::{
    BundleState, DesktopSnapshotFields, Dimensions, ExportMeta, FileFields, FileHash, FilePolicy,
    FileStorage, HandoffRecord, HandoffStatus, HandoffTimestamps, ImageFields, ImageProvenance,
    ImageRole, Item, ItemBody, Manifest, Producer, Rect, TextContent, TextFields, TextRole,
    Transport, UrlFields, UrlSource, Warning, SCHEMA_VERSION,
};

/// Layout of a `.ctx` portable archive.
pub const ARCHIVE_FORMAT: &str = "ctx-archive";
/// Current `.ctx` archive layout version.
pub const ARCHIVE_VERSION: u32 = 1;

/// Generate a stable bundle id: `2026-06-24T10-41-03Z_7hf3k2` plus an
/// optional slug suffix (`..._7hf3k2_build-slides`).
///
/// The timestamp is UTC, RFC3339-ish with colons replaced by dashes so the id
/// is filesystem-safe.
pub fn generate_id(slug: Option<&str>) -> String {
    let now = OffsetDateTime::now_utc();
    // Format: 2026-06-24T10-41-03Z
    let ts = format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}Z",
        year = now.year(),
        month = now.month() as u8,
        day = now.day(),
        hour = now.hour(),
        minute = now.minute(),
        second = now.second()
    );
    let short = short_id();
    let slug_part = slug
        .map(slugify)
        .filter(|s| !s.is_empty())
        .map(|s| format!("_{s}"))
        .unwrap_or_default();
    format!("{ts}_{short}{slug_part}")
}

/// Generate a short, lowercase id (6 hex chars from a UUID v4).
fn short_id() -> String {
    let uuid = Uuid::new_v4().simple().to_string();
    uuid[..6].to_string()
}

/// Generate a short id for an item, prefixed (e.g. `img_a1b2c3`).
pub fn item_id(prefix: &str) -> String {
    format!("{prefix}_{}", short_id())
}

/// Slugify a string: lowercase, alphanumerics and dashes only, trimmed.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Compute a SHA-256 hex digest of a file's bytes.
pub fn sha256_file(path: &Path) -> std::io::Result<FileHash> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; 65536];
    let mut hasher = Sha256::new();
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(FileHash {
        algorithm: "sha256".to_string(),
        value: hasher.finalize_hex(),
    })
}

// Minimal, dependency-free SHA-256 used for file content addressing. Keeping it
// internal avoids pulling another crate into ctx-core for hashing alone.
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    length_bits: u64,
}

impl Sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            length_bits: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.length_bits = self.length_bits.wrapping_add((data.len() as u64) * 8);
        let mut data = data;
        if self.buffer_len > 0 {
            let need = 64 - self.buffer_len;
            let take = need.min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take]
                .copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.process_block(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    fn finalize_hex(mut self) -> String {
        let mut pad = [0u8; 64];
        pad[0] = 0x80;
        let buf = self.buffer;
        let len = self.buffer_len;
        let mut block = [0u8; 64];
        block[..len].copy_from_slice(&buf[..len]);
        if len + 1 + 8 <= 64 {
            block[len] = 0x80;
            block[56..64].copy_from_slice(&self.length_bits.to_be_bytes());
            self.process_block(&block);
        } else {
            block[len] = 0x80;
            self.process_block(&block);
            let mut block2 = [0u8; 64];
            block2[56..64].copy_from_slice(&self.length_bits.to_be_bytes());
            self.process_block(&block2);
        }
        let _ = pad;
        let mut out = String::with_capacity(64);
        for word in self.state {
            out.push_str(&format!("{word:08x}"));
        }
        out
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];
        for (i, &wi) in w.iter().enumerate() {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(Self::K[i])
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("bundle not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to serialize manifest: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("item {0} not found in bundle")]
    ItemNotFound(String),
    #[error("item {0} is not an image")]
    NotAnImage(String),
    #[error("crop rect {rect} is outside the source image ({width}x{height})")]
    CropOutOfBounds {
        rect: String,
        width: u32,
        height: u32,
    },
    #[error("source image has no dimensions")]
    NoDimensions,
    #[error("source image has no stored file")]
    NoPath,
    #[error("{0}")]
    Other(String),
}

/// The on-disk bundle store rooted at `bundle_dir`.
#[derive(Debug, Clone)]
pub struct BundleStore {
    root: PathBuf,
}

impl BundleStore {
    /// Create a store backed by the given root directory (created if missing).
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Root directory of the store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory for a single bundle id.
    pub fn bundle_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Path to a bundle's manifest.
    pub fn manifest_path(&self, id: &str) -> PathBuf {
        self.bundle_dir(id).join("manifest.json")
    }

    /// Create a new draft bundle and persist it.
    pub fn create(
        &self,
        slug: Option<&str>,
        producer: Producer,
    ) -> Result<(Manifest, BundleStoreMut), StoreError> {
        let id = generate_id(slug);
        let manifest = {
            let mut m = Manifest::new(id.clone(), producer);
            m.slug = slug.map(slugify).filter(|s| !s.is_empty());
            m
        };
        let dir = self.bundle_dir(&id);
        fs::create_dir_all(dir.join("images"))?;
        fs::create_dir_all(dir.join("text"))?;
        fs::create_dir_all(dir.join("files"))?;
        fs::create_dir_all(dir.join("snapshots"))?;
        fs::create_dir_all(dir.join("ocr"))?;
        self.save(&manifest)?;
        Ok((manifest, BundleStoreMut::new(self.clone(), id)))
    }

    /// Load a bundle's manifest.
    pub fn load(&self, id: &str) -> Result<Manifest, StoreError> {
        let path = self.manifest_path(id);
        if !path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let content = fs::read_to_string(path)?;
        let manifest: Manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Persist a manifest to disk.
    pub fn save(&self, manifest: &Manifest) -> Result<(), StoreError> {
        let dir = self.bundle_dir(&manifest.id);
        fs::create_dir_all(&dir)?;
        let path = self.manifest_path(&manifest.id);
        let mut file = fs::File::create(&path)?;
        let json = serde_json::to_vec_pretty(manifest)?;
        file.write_all(&json)?;
        Ok(())
    }

    /// Render and write the agent-readable `context.md` for a bundle.
    pub fn write_context_md(&self, manifest: &Manifest) -> Result<PathBuf, StoreError> {
        let dir = self.bundle_dir(&manifest.id);
        let path = dir.join("context.md");
        let content = render_context_md(manifest, &dir);
        fs::write(&path, content)?;
        Ok(path)
    }

    /// List bundle ids known to the store.
    pub fn list(&self) -> Result<Vec<String>, StoreError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if self.manifest_path(&name).exists() {
                ids.push(name);
            }
        }
        ids.sort();
        Ok(ids)
    }
}

/// A scoped mutator for a single open bundle. Owns the in-memory manifest and
/// flushes it to disk on [`save`]/[`finalize`].
///
/// [`save`]: BundleStoreMut::save
/// [`finalize`]: BundleStoreMut::finalize
#[derive(Debug, Clone)]
pub struct BundleStoreMut {
    store: BundleStore,
    bundle_id: String,
}

impl BundleStoreMut {
    /// Open an existing bundle by id for mutation.
    pub fn new(store: BundleStore, bundle_id: String) -> Self {
        Self { store, bundle_id }
    }

    /// The bundle id.
    pub fn id(&self) -> &str {
        &self.bundle_id
    }

    /// The bundle directory.
    pub fn dir(&self) -> PathBuf {
        self.store.bundle_dir(&self.bundle_id)
    }

    /// Borrow the underlying store.
    pub fn store(&self) -> &BundleStore {
        &self.store
    }

    /// Load the current manifest.
    pub fn manifest(&self) -> Result<Manifest, StoreError> {
        self.store.load(&self.bundle_id)
    }

    /// Apply a mutation to the manifest and persist it (also refreshing
    /// `context.md`).
    pub fn update<F, R>(&mut self, f: F) -> Result<R, StoreError>
    where
        F: FnOnce(&mut Manifest) -> R,
    {
        let mut manifest = self.store.load(&self.bundle_id)?;
        let result = f(&mut manifest);
        manifest.touch();
        self.store.save(&manifest)?;
        // Best-effort context.md; never fail the update on render errors.
        let _ = self.store.write_context_md(&manifest);
        Ok(result)
    }

    /// Persist an already-modified manifest directly.
    pub fn save(&mut self, manifest: &mut Manifest) -> Result<(), StoreError> {
        manifest.touch();
        self.store.save(manifest)?;
        let _ = self.store.write_context_md(manifest);
        Ok(())
    }

    // -- item helpers -------------------------------------------------------

    /// Add a text item.
    pub fn add_text(
        &mut self,
        role: TextRole,
        text: &str,
        note: Option<&str>,
    ) -> Result<Item, StoreError> {
        let item_id = item_id("txt");
        let inline_text = text.to_string();
        let item = Item::new(
            item_id.clone(),
            ItemBody::Text(TextFields {
                role,
                content: TextContent::Inline {
                    text: inline_text.clone(),
                },
                source: Default::default(),
                note: note.map(str::to_string),
            }),
        );
        self.update(|m| m.items.push(item.clone()))?;
        Ok(item)
    }

    /// Add a URL item.
    pub fn add_url(
        &mut self,
        url: &str,
        title: Option<&str>,
        source: UrlSource,
    ) -> Result<Item, StoreError> {
        let item = Item::new(
            item_id("url"),
            ItemBody::Url(UrlFields {
                url: url.to_string(),
                title: title.map(str::to_string),
                source,
            }),
        );
        self.update(|m| m.items.push(item.clone()))?;
        Ok(item)
    }

    /// Add a file item, honoring the requested storage policy.
    pub fn add_file(
        &mut self,
        path: &Path,
        policy: FilePolicy,
    ) -> Result<Item, StoreError> {
        // Reference policy only stores the path; the file need not exist yet.
        // Copy/Auto require reading metadata and bytes.
        let want_copy = match policy {
            FilePolicy::Reference => false,
            FilePolicy::Copy => true,
            FilePolicy::Auto => true,
        };

        let meta = if want_copy {
            Some(fs::metadata(path)?)
        } else {
            fs::metadata(path).ok()
        };
        let size_bytes = meta.as_ref().map(|m| m.len());
        let mime = mime_for(path);
        let hash = if want_copy { sha256_file(path).ok() } else { None };

        let do_copy = match policy {
            FilePolicy::Auto => meta.map(|m| !m.is_dir()).unwrap_or(false),
            FilePolicy::Copy => true,
            FilePolicy::Reference => false,
        };

        let storage = if do_copy {
            let bundle_path = format!(
                "files/{}",
                sanitize_filename(&path.file_name().unwrap_or_default().to_string_lossy())
            );
            let dest = self.dir().join(&bundle_path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest)?;
            FileStorage::Copied {
                bundle_path,
                original_path: path.to_string_lossy().into_owned(),
            }
        } else {
            FileStorage::Referenced {
                path: path.to_string_lossy().into_owned(),
            }
        };

        let item = Item::new(
            item_id("file"),
            ItemBody::File(FileFields {
                storage,
                size_bytes,
                mime,
                hash,
                note: None,
            }),
        );
        self.update(|m| m.items.push(item.clone()))?;
        Ok(item)
    }

    /// Add a desktop snapshot item from a pre-written capture JSON file. The
    /// snapshot is copied into the bundle.
    pub fn add_snapshot(&mut self, capture_path: &Path) -> Result<Item, StoreError> {
        let bundle_path = "snapshots/desktop.json".to_string();
        let dest = self.dir().join(&bundle_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(capture_path, &dest)?;
        let item = Item::new(
            item_id("snap"),
            ItemBody::DesktopSnapshot(DesktopSnapshotFields {
                path: bundle_path,
                format: Some("ctx-capture-envelope".to_string()),
                note: None,
            }),
        );
        self.update(|m| m.items.push(item.clone()))?;
        Ok(item)
    }

    /// Add an image item from already-written bytes on disk (within the
    /// bundle), with given role, dimensions, and provenance.
    pub fn add_image_item(
        &mut self,
        role: ImageRole,
        rel_path: &str,
        dimensions: Option<Dimensions>,
        provenance: ImageProvenance,
        cropped_from: Option<String>,
    ) -> Result<Item, StoreError> {
        let item = Item::new(
            item_id(match role {
                ImageRole::Screenshot => "img",
                ImageRole::Crop => "img",
                ImageRole::Pasted => "img",
            }),
            ItemBody::Image(ImageFields {
                role,
                path: rel_path.to_string(),
                mime: mime_for_extension(Path::new(rel_path)),
                dimensions,
                provenance,
                ocr_refs: Vec::new(),
                cropped_from,
                note: None,
            }),
        );
        self.update(|m| m.items.push(item.clone()))?;
        Ok(item)
    }

    /// Crop an existing image item by `x,y,w,h` and add the crop as a new image
    /// item. Returns the new item plus the in-memory crop bytes.
    pub fn crop_image(
        &mut self,
        source_item_id: &str,
        rect: Rect,
    ) -> Result<(Item, image::DynamicImage), StoreError> {
        let manifest = self.store.load(&self.bundle_id)?;
        let source = manifest
            .find_item(source_item_id)
            .ok_or_else(|| StoreError::ItemNotFound(source_item_id.to_string()))?;
        let ImageFields {
            path,
            dimensions,
            cropped_from,
            ..
        } = match &source.body {
            ItemBody::Image(f) => f,
            _ => return Err(StoreError::NotAnImage(source_item_id.to_string())),
        };
        let dims = dimensions.ok_or(StoreError::NoDimensions)?;
        let src_abs = self.dir().join(path);
        let img = image::open(&src_abs)?;
        validate_rect(&rect, dims.width, dims.height)?;

        let cropped = img.crop_imm(rect.x, rect.y, rect.width, rect.height);
        let new_id = item_id("img");
        let rel = format!("images/crop-{}.jpg", new_id);
        let dest = self.dir().join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        cropped.save_with_format(&dest, image::ImageFormat::Jpeg)?;

        let crop_item = Item::new(
            new_id.clone(),
            ItemBody::Image(ImageFields {
                role: ImageRole::Crop,
                path: rel,
                mime: Some("image/jpeg".to_string()),
                dimensions: Some(Dimensions {
                    width: cropped.width(),
                    height: cropped.height(),
                }),
                provenance: ImageProvenance {
                    rect: Some(rect),
                    captured_at: Some(OffsetDateTime::now_utc()),
                    ..Default::default()
                },
                ocr_refs: Vec::new(),
                cropped_from: Some(source_item_id.to_string()),
                note: None,
            }),
        );
        // Preserve the source's cropped_from chain defensively; a crop's parent
        // is always the immediate source item id.
        let _ = cropped_from;

        self.update(|m| m.items.push(crop_item.clone()))?;
        Ok((crop_item, cropped))
    }

    /// Attach an OCR text item to an image, recording the reference on the
    /// image and writing the recognized text into the bundle.
    pub fn attach_ocr(
        &mut self,
        image_item_id: &str,
        text: &str,
    ) -> Result<Item, StoreError> {
        let ocr_item_id = item_id("txt");
        // Write the OCR text to a file within the bundle for durability.
        let rel = format!("ocr/{}.txt", ocr_item_id);
        let dest = self.dir().join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, text)?;
        let ocr_item = Item::new(
            ocr_item_id.clone(),
            ItemBody::Text(TextFields {
                role: TextRole::Ocr,
                content: TextContent::File { path: rel },
                source: Default::default(),
                note: None,
            }),
        );
        self.update(|m| {
            m.items.push(ocr_item.clone());
            if let Some(ItemBody::Image(img)) = m.find_item_mut(image_item_id).map(|i| &mut i.body)
            {
                img.ocr_refs.push(ocr_item.id.clone());
            }
        })?;
        Ok(ocr_item)
    }

    /// Record a handoff for the bundle.
    pub fn add_handoff(
        &mut self,
        target_id: &str,
        transport: Transport,
        status: HandoffStatus,
    ) -> Result<HandoffRecord, StoreError> {
        let mut record = HandoffRecord {
            target_id: target_id.to_string(),
            transport,
            bundle_id: self.bundle_id.clone(),
            status,
            timestamps: HandoffTimestamps::default(),
            session_affinity_key: None,
            target_meta: Default::default(),
            output: None,
            log: None,
            note: None,
        };
        match status {
            HandoffStatus::Queued => record.timestamps.queued_at = Some(OffsetDateTime::now_utc()),
            HandoffStatus::Sent => record.timestamps.sent_at = Some(OffsetDateTime::now_utc()),
            HandoffStatus::Delivered => {
                record.timestamps.delivered_at = Some(OffsetDateTime::now_utc())
            }
            HandoffStatus::Failed => record.timestamps.failed_at = Some(OffsetDateTime::now_utc()),
        }
        let cloned = record.clone();
        self.update(|m| {
            if matches!(status, HandoffStatus::Sent) && m.state == BundleState::Draft {
                m.state = BundleState::Sent;
            }
            m.handoffs.push(cloned);
        })?;
        Ok(record)
    }

    /// Record a warning on the bundle.
    pub fn add_warning(&mut self, warning: Warning) -> Result<(), StoreError> {
        self.update(|m| {
            m.warnings.push(warning);
        })
    }

    /// Persist export metadata after a `.ctx` export.
    pub fn set_export(&mut self, export: ExportMeta) -> Result<(), StoreError> {
        self.update(|m| {
            m.export = Some(export);
        })
    }

    /// Record an unresolved external reference.
    pub fn add_unresolved(&mut self, item_id: &str, path: &str, reason: &str) -> Result<(), StoreError> {
        let r = crate::manifest::UnresolvedRef {
            item_id: item_id.to_string(),
            path: path.to_string(),
            reason: reason.to_string(),
        };
        self.update(|m| {
            m.unresolved_refs.push(r);
        })
    }
}

fn validate_rect(rect: &Rect, width: u32, height: u32) -> Result<(), StoreError> {
    let fits = rect.x.checked_add(rect.width).is_some_and(|x| x <= width)
        && rect.y.checked_add(rect.height).is_some_and(|y| y <= height);
    if fits {
        Ok(())
    } else {
        Err(StoreError::CropOutOfBounds {
            rect: format!(
                "x={},y={},w={},h={}",
                rect.x, rect.y, rect.width, rect.height
            ),
            width,
            height,
        })
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn mime_for(path: &Path) -> Option<String> {
    mime_for_extension(path)
}

fn mime_for_extension(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => Some("image/jpeg".to_string()),
        Some("png") => Some("image/png".to_string()),
        Some("gif") => Some("image/gif".to_string()),
        Some("webp") => Some("image/webp".to_string()),
        Some("json") => Some("application/json".to_string()),
        Some("md") => Some("text/markdown".to_string()),
        Some("txt") => Some("text/plain".to_string()),
        Some("html") | Some("htm") => Some("text/html".to_string()),
        Some("pdf") => Some("application/pdf".to_string()),
        _ => None,
    }
}

/// Render an agent-readable Markdown summary of the bundle.
pub fn render_context_md(manifest: &Manifest, bundle_dir: &Path) -> String {
    let mut md = String::new();
    md.push_str("# Context bundle\n\n");
    md.push_str(&format!("- **Bundle id:** `{}`\n", manifest.id));
    md.push_str(&format!("- **State:** {}\n", format_state(manifest.state)));
    if let Some(slug) = &manifest.slug {
        md.push_str(&format!("- **Slug:** {slug}\n"));
    }
    md.push_str(&format!(
        "- **Producer:** {}\n",
        manifest.producer.name
    ));
    md.push_str(&format!(
        "- **Bundle path:** `{}`\n",
        bundle_dir.display()
    ));
    md.push('\n');

    // Task / primary text first.
    if let Some(task) = manifest.items.iter().find(|i| matches!(
        &i.body,
        ItemBody::Text(TextFields {
            role: TextRole::Task, ..
        })
    )) {
        md.push_str("## Task\n\n");
        append_text(&mut md, task, bundle_dir);
        md.push('\n');
    }

    // Other text items (notes, clipboard, ocr, accessibility).
    let other_text: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|i| {
            matches!(
                &i.body,
                ItemBody::Text(TextFields { role, .. }) if *role != TextRole::Task
            )
        })
        .collect();
    if !other_text.is_empty() {
        md.push_str("## Text\n\n");
        for item in other_text {
            append_text(&mut md, item, bundle_dir);
        }
        md.push('\n');
    }

    // Images.
    let images: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|i| matches!(i.body, ItemBody::Image { .. }))
        .collect();
    if !images.is_empty() {
        md.push_str("## Images\n\n");
        for item in images {
            if let ItemBody::Image(img) = &item.body {
                let abs = bundle_dir.join(&img.path);
                md.push_str(&format!("- **{}** `{}`\n", item.id, img.path));
                md.push_str(&format!("  - role: {:?}\n", img.role));
                if let Some(d) = &img.dimensions {
                    md.push_str(&format!("  - size: {}x{}\n", d.width, d.height));
                }
                md.push_str(&format!("  - file: `{}`\n", abs.display()));
                if let Some(from) = &img.cropped_from {
                    md.push_str(&format!("  - cropped from: `{from}`\n"));
                }
                if !img.ocr_refs.is_empty() {
                    md.push_str(&format!("  - ocr: {}\n", img.ocr_refs.join(", ")));
                }
            }
        }
        md.push('\n');
    }

    // Files.
    let files: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|i| matches!(i.body, ItemBody::File { .. }))
        .collect();
    if !files.is_empty() {
        md.push_str("## Files\n\n");
        for item in files {
            if let ItemBody::File(f) = &item.body {
                md.push_str(&format!("- **{}**\n", item.id));
                md.push_str(&format!("  - source: `{}`\n", f.source_path()));
                match &f.storage {
                    FileStorage::Copied { bundle_path, .. } => {
                        md.push_str(&format!("  - in bundle: `{}`\n", bundle_path));
                    }
                    FileStorage::Referenced { .. } => {
                        md.push_str("  - referenced in place (not copied)\n");
                    }
                }
                if let Some(size) = f.size_bytes {
                    md.push_str(&format!("  - size: {size} bytes\n"));
                }
            }
        }
        md.push('\n');
    }

    // URLs.
    let urls: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|i| matches!(i.body, ItemBody::Url { .. }))
        .collect();
    if !urls.is_empty() {
        md.push_str("## URLs\n\n");
        for item in urls {
            if let ItemBody::Url(u) = &item.body {
                md.push_str(&format!("- **{}** <{}>\n", item.id, u.url));
                if let Some(title) = &u.title {
                    md.push_str(&format!("  - title: {title}\n"));
                }
            }
        }
        md.push('\n');
    }

    // Desktop snapshots.
    let snaps: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|i| matches!(i.body, ItemBody::DesktopSnapshot { .. }))
        .collect();
    if !snaps.is_empty() {
        md.push_str("## Desktop snapshots\n\n");
        for item in snaps {
            if let ItemBody::DesktopSnapshot(s) = &item.body {
                md.push_str(&format!("- **{}** `{}`\n", item.id, s.path));
            }
        }
        md.push('\n');
    }

    // Handoffs.
    if !manifest.handoffs.is_empty() {
        md.push_str("## Handoffs\n\n");
        for h in &manifest.handoffs {
            md.push_str(&format!(
                "- target `{}` via {:?}: {:?}\n",
                h.target_id, h.transport, h.status
            ));
        }
        md.push('\n');
    }

    // Warnings.
    if !manifest.warnings.is_empty() {
        md.push_str("## Warnings\n\n");
        for w in &manifest.warnings {
            md.push_str(&format!("- [{}] {}\n", w.code, w.message));
        }
        md.push('\n');
    }

    md.push_str(&format!(
        "_schema_version {SCHEMA_VERSION}_\n"
    ));
    md
}

fn format_state(state: BundleState) -> &'static str {
    match state {
        BundleState::Draft => "draft",
        BundleState::Sent => "sent",
        BundleState::Archived => "archived",
        BundleState::Cancelled => "cancelled",
    }
}

fn append_text(md: &mut String, item: &Item, bundle_dir: &Path) {
    if let ItemBody::Text(t) = &item.body {
        md.push_str(&format!("**{}** (role: {:?})\n\n", item.id, t.role));
        match &t.content {
            TextContent::Inline { text } => {
                md.push_str("``````text\n");
                md.push_str(text);
                if !text.ends_with('\n') {
                    md.push('\n');
                }
                md.push_str("``````\n\n");
            }
            TextContent::File { path } => {
                let abs = bundle_dir.join(path);
                md.push_str(&format!("file: `{}`\n\n", abs.display()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_format_no_slug() {
        let id = generate_id(None);
        // 2026-06-24T10-41-03Z_7hf3k2
        assert!(
            id.starts_with("20") && id.contains('T') && !id.ends_with('Z'),
            "id={id}"
        );
        let rest = id.split('Z').nth(1).unwrap();
        assert!(rest.starts_with('_'), "rest={rest}");
        let parts: Vec<&str> = rest[1..].splitn(2, '_').collect();
        assert_eq!(parts[0].len(), 6, "short id len: {}", parts[0]);
    }

    #[test]
    fn generate_id_format_with_slug() {
        let id = generate_id(Some("Build Slides!"));
        assert!(id.ends_with("_build-slides"), "id={id}");
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify("Build Slides!"), "build-slides");
        assert_eq!(slugify("  a  b  "), "a-b");
        assert_eq!(slugify("___"), "");
    }

    #[test]
    fn sha256_known_vector() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb924...
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.txt");
        fs::write(&path, b"").unwrap();
        let hash = sha256_file(&path).unwrap();
        assert_eq!(hash.algorithm, "sha256");
        assert_eq!(
            hash.value,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_known_vector_abc() {
        // sha256("abc")
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("abc.txt");
        fs::write(&path, b"abc").unwrap();
        let hash = sha256_file(&path).unwrap();
        assert_eq!(
            hash.value,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn store_create_load_save_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, muthandle) =
            store.create(Some("roundtrip"), Producer::new("ctx")).unwrap();
        assert_eq!(manifest.schema_version, SCHEMA_VERSION);
        assert_eq!(manifest.state, BundleState::Draft);
        assert!(manifest.slug.as_deref() == Some("roundtrip"));
        assert!(store.manifest_path(&manifest.id).exists());
        let _ = muthandle;

        let loaded = store.load(&manifest.id).unwrap();
        assert_eq!(loaded.id, manifest.id);
    }

    #[test]
    fn store_add_items_and_context_md() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) = store.create(Some("demo"), Producer::new("omni")).unwrap();

        let task = h
            .add_text(TextRole::Task, "Build a deck about bundles", None)
            .unwrap();
        let url = h
            .add_url(
                "https://example.com/spec",
                Some("Spec"),
                UrlSource {
                    app: Some("Safari".to_string()),
                    window_title: None,
                },
            )
            .unwrap();
        let file_tmp = tempfile::tempdir().unwrap();
        let file_path = file_tmp.path().join("notes.md");
        fs::write(&file_path, "# Notes\nhello").unwrap();
        let file = h.add_file(&file_path, FilePolicy::Auto).unwrap();

        let md_path = store.write_context_md(&store.load(&manifest.id).unwrap()).unwrap();
        let md = fs::read_to_string(&md_path).unwrap();
        assert!(md.contains("Build a deck about bundles"));
        assert!(md.contains("https://example.com/spec"));
        assert!(md.contains("notes.md"));
        assert!(md.contains("in bundle"));

        // handoff transitions to sent
        let rec = h
            .add_handoff("pi-slides", Transport::LocalProcess, HandoffStatus::Sent)
            .unwrap();
        assert_eq!(rec.status, HandoffStatus::Sent);
        let loaded = store.load(&manifest.id).unwrap();
        assert_eq!(loaded.state, BundleState::Sent);
        assert_eq!(loaded.handoffs.len(), 1);

        // ids are stable
        assert!(store.load(&task.id).is_err());
        assert!(store.load(&url.id).is_err());
        assert!(store.load(&file.id).is_err());
    }

    #[test]
    fn store_crop_image() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) =
            store.create(Some("crop"), Producer::new("ctx")).unwrap();

        // Create a 100x100 solid image as the source.
        let src_rel = "images/source.jpg";
        let src_abs = store.bundle_dir(&manifest.id).join(src_rel);
        fs::create_dir_all(src_abs.parent().unwrap()).unwrap();
        let img = image::RgbImage::from_pixel(100, 100, image::Rgb([200, 50, 50]));
        image::DynamicImage::ImageRgb8(img)
            .save_with_format(&src_abs, image::ImageFormat::Jpeg)
            .unwrap();

        let source_item = h
            .add_image_item(
                ImageRole::Screenshot,
                src_rel,
                Some(Dimensions {
                    width: 100,
                    height: 100,
                }),
                ImageProvenance::default(),
                None,
            )
            .unwrap();

        let (crop_item, cropped) = h
            .crop_image(&source_item.id, Rect::new(10, 10, 40, 40))
            .unwrap();
        assert_eq!(cropped.width(), 40);
        assert_eq!(cropped.height(), 40);
        let loaded = store.load(&manifest.id).unwrap();
        let body = loaded.find_item(&crop_item.id).unwrap();
        match &body.body {
            ItemBody::Image(ImageFields {
                role,
                cropped_from,
                dimensions,
                ..
            }) => {
                assert_eq!(*role, ImageRole::Crop);
                assert_eq!(*cropped_from.as_deref().unwrap(), source_item.id);
                assert_eq!(dimensions.as_ref().unwrap().width, 40);
            }
            _ => panic!("expected image"),
        }
    }

    #[test]
    fn store_crop_out_of_bounds_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) = store.create(None, Producer::new("ctx")).unwrap();

        let src_rel = "images/source.jpg";
        let src_abs = store.bundle_dir(&manifest.id).join(src_rel);
        fs::create_dir_all(src_abs.parent().unwrap()).unwrap();
        let img = image::RgbImage::from_pixel(50, 50, image::Rgb([10, 20, 30]));
        image::DynamicImage::ImageRgb8(img)
            .save_with_format(&src_abs, image::ImageFormat::Jpeg)
            .unwrap();
        let source_item = h
            .add_image_item(
                ImageRole::Screenshot,
                src_rel,
                Some(Dimensions {
                    width: 50,
                    height: 50,
                }),
                ImageProvenance::default(),
                None,
            )
            .unwrap();

        let err = h
            .crop_image(&source_item.id, Rect::new(0, 0, 60, 10))
            .unwrap_err();
        assert!(matches!(err, StoreError::CropOutOfBounds { .. }));
    }

    #[test]
    fn store_attach_ocr_records_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) = store.create(None, Producer::new("ctx")).unwrap();
        let img = h
            .add_image_item(
                ImageRole::Screenshot,
                "images/x.jpg",
                Some(Dimensions {
                    width: 10,
                    height: 10,
                }),
                ImageProvenance::default(),
                None,
            )
            .unwrap();
        let ocr = h.attach_ocr(&img.id, "recognized text").unwrap();
        let loaded = store.load(&manifest.id).unwrap();
        let img_back = loaded.find_item(&img.id).unwrap();
        match &img_back.body {
            ItemBody::Image(ImageFields { ocr_refs, .. }) => {
                assert_eq!(ocr_refs, &vec![ocr.id.clone()]);
            }
            _ => panic!("expected image"),
        }
        let ocr_back = loaded.find_item(&ocr.id).unwrap();
        assert!(matches!(ocr_back.body, ItemBody::Text(TextFields { role: TextRole::Ocr, .. })));
    }

    #[test]
    fn store_list_returns_bundles() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (m1, _) = store.create(Some("a"), Producer::new("ctx")).unwrap();
        let (m2, _) = store.create(Some("b"), Producer::new("ctx")).unwrap();
        let ids = store.list().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&m1.id));
        assert!(ids.contains(&m2.id));
    }
}
