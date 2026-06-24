//! `ctx bundle ...` subcommand implementation for Agent Handoff workflows.
//!
//! Every command emits stable, machine-readable JSON so it can be driven by
//! scripts and a future daemon/API. Commands are defensive: failed capture/OCR
//! records warnings on the bundle instead of corrupting it.

use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use ctx_core::bundle_capture::{
    self, ocr_item, write_desktop_snapshot, CaptureHints, CaptureTarget, OcrBackend,
};
use ctx_core::config::{self, AppConfig};
use ctx_core::export::export_ctx;
use ctx_core::manifest::{
    FilePolicy, HandoffStatus, Producer, Rect, TextRole, Transport, UrlSource,
};
use ctx_core::store::{BundleStore, BundleStoreMut};

/// `ctx bundle` subcommands.
#[derive(Debug, Args)]
pub struct BundleCli {
    #[command(subcommand)]
    pub command: BundleCommand,
}

#[derive(Debug, Subcommand)]
pub enum BundleCommand {
    /// Create a new draft context bundle.
    Create {
        /// Producer name (e.g. `omni`, `ctx`).
        #[arg(long, default_value = "ctx")]
        producer: String,
        /// Optional slug appended to the bundle id.
        #[arg(long)]
        slug: Option<String>,
    },
    /// Add a screenshot item (frontmost window / display / all displays).
    AddScreenshot {
        bundle_id: String,
        /// Capture mode.
        #[arg(long, value_enum, default_value_t = CaptureModeCli::Frontmost)]
        mode: CaptureModeCli,
        /// URL associated with the captured element, if known.
        #[arg(long)]
        url: Option<String>,
        /// Override screenshot JPEG quality (0-100).
        #[arg(long)]
        quality: Option<u8>,
    },
    /// Crop an existing image item by rectangle and add the crop as a new item.
    CropImage {
        bundle_id: String,
        /// Source image item id.
        #[arg(long)]
        item: String,
        /// Crop rect as `x,y,w,h`.
        #[arg(long)]
        rect: String,
    },
    /// Add a file item (copied or referenced).
    AddFile {
        bundle_id: String,
        /// Path to the file.
        path: PathBuf,
        /// Storage policy.
        #[arg(long, value_enum, default_value_t = FilePolicyCli::Auto)]
        policy: FilePolicyCli,
    },
    /// Add a text item (task / note / clipboard / ocr). Reads from --file
    /// (use `-` for stdin) or --text.
    AddText {
        bundle_id: String,
        /// Text role.
        #[arg(long, value_enum, default_value_t = TextRoleCli::Task)]
        role: TextRoleCli,
        /// Read text from a file (`-` for stdin). Mutually exclusive with --text.
        #[arg(long)]
        file: Option<String>,
        /// Inline text.
        #[arg(long)]
        text: Option<String>,
    },
    /// Add a URL item.
    AddUrl {
        bundle_id: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        window: Option<String>,
    },
    /// Add a desktop snapshot (full ctx capture envelope).
    AddSnapshot { bundle_id: String },
    /// Run OCR on an image item using the configured backend.
    Ocr {
        bundle_id: String,
        #[arg(long)]
        item: String,
    },
    /// Record a handoff / dispatch for the bundle.
    Handoff {
        bundle_id: String,
        #[arg(long)]
        target: String,
        #[arg(long, value_enum, default_value_t = HandoffStatusCli::Sent)]
        status: HandoffStatusCli,
        #[arg(long, value_enum, default_value_t = TransportCli::LocalProcess)]
        transport: TransportCli,
    },
    /// Export a bundle to a portable `.ctx` zip archive.
    Export {
        bundle_id: String,
        #[arg(long, value_enum, default_value_t = ExportFormatCli::Ctx)]
        format: ExportFormatCli,
    },
    /// Show a bundle's manifest.
    Show { bundle_id: String },
    /// List all bundles in the store.
    List,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CaptureModeCli {
    Frontmost,
    Display,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FilePolicyCli {
    Auto,
    Copy,
    Reference,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TextRoleCli {
    Task,
    Note,
    Clipboard,
    Ocr,
    Accessibility,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HandoffStatusCli {
    Queued,
    Sent,
    Delivered,
    Failed,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TransportCli {
    LocalProcess,
    Tmux,
    OqtoApi,
    RemoteAgent,
    Queue,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormatCli {
    Ctx,
}

impl BundleCommand {
    pub fn run(&self, cfg: &AppConfig) -> anyhow::Result<()> {
        let store = BundleStore::new(&cfg.output.bundle_dir)?;
        match self {
            Self::Create { producer, slug } => {
                let (_manifest, h) = store.create(slug.as_deref(), Producer::new(producer))?;
                let manifest = store.load(h.id())?;
                emit_bundle(&manifest);
            }
            Self::AddScreenshot {
                bundle_id,
                mode,
                url,
                quality,
            } => {
                let target = match mode {
                    CaptureModeCli::Frontmost => CaptureTarget::Frontmost,
                    CaptureModeCli::Display => CaptureTarget::Display,
                    CaptureModeCli::All => CaptureTarget::All,
                };
                let hints = CaptureHints {
                    url: url.clone(),
                    image_quality: *quality,
                };
                let mut h = open(&store, bundle_id)?;
                match bundle_capture::add_screenshot(&mut h, target, hints) {
                    Ok(captured) => {
                        let items: Vec<_> = captured
                            .iter()
                            .map(|c| serde_json::json!({
                                "item_id": c.item.id,
                                "path": item_path(&h, &c.item.id),
                                "role": "screenshot",
                                "dimensions": {
                                    "width": c.dimensions.width,
                                    "height": c.dimensions.height,
                                },
                            }))
                            .collect();
                        print_json(&serde_json::json!({
                            "bundle_id": h.id(),
                            "items": items,
                        }));
                    }
                    Err(err) => {
                        let warning = ctx_core::manifest::Warning::new(
                            "capture_failed",
                            err.to_string(),
                        );
                        h.add_warning(warning)?;
                        print_json(&serde_json::json!({
                            "bundle_id": h.id(),
                            "error": err.to_string(),
                            "warning": "capture_failed",
                        }));
                    }
                }
            }
            Self::CropImage {
                bundle_id,
                item,
                rect,
            } => {
                let rect = parse_rect(rect)?;
                let mut h = open(&store, bundle_id)?;
                let (new_item, _img) = h.crop_image(item, rect)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "item_id": new_item.id,
                    "path": item_path(&h, &new_item.id),
                    "role": "crop",
                    "cropped_from": item,
                }));
            }
            Self::AddFile {
                bundle_id,
                path,
                policy,
            } => {
                let policy = match policy {
                    FilePolicyCli::Auto => FilePolicy::Auto,
                    FilePolicyCli::Copy => FilePolicy::Copy,
                    FilePolicyCli::Reference => FilePolicy::Reference,
                };
                let mut h = open(&store, bundle_id)?;
                let item = h.add_file(path, policy)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "item_id": item.id,
                    "source": path.display().to_string(),
                    "kind": item.kind(),
                }));
            }
            Self::AddText {
                bundle_id,
                role,
                file,
                text,
            } => {
                let role = match role {
                    TextRoleCli::Task => TextRole::Task,
                    TextRoleCli::Note => TextRole::Note,
                    TextRoleCli::Clipboard => TextRole::Clipboard,
                    TextRoleCli::Ocr => TextRole::Ocr,
                    TextRoleCli::Accessibility => TextRole::Accessibility,
                };
                // Clipboard role: pull from the system clipboard unless text
                // is explicitly provided.
                let content = if role == TextRole::Clipboard && text.is_none() && file.is_none() {
                    bundle_capture::read_clipboard_text()
                        .ok_or_else(|| anyhow::anyhow!("no clipboard text available"))?
                } else {
                    resolve_text(text.as_deref(), file.as_deref())?
                };
                let mut h = open(&store, bundle_id)?;
                let item = h.add_text(role, &content, None)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "item_id": item.id,
                    "kind": "text",
                }));
            }
            Self::AddUrl {
                bundle_id,
                url,
                title,
                app,
                window,
            } => {
                let source = UrlSource {
                    app: app.clone(),
                    window_title: window.clone(),
                };
                let mut h = open(&store, bundle_id)?;
                let item = h.add_url(url, title.as_deref(), source)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "item_id": item.id,
                    "kind": "url",
                    "url": url,
                }));
            }
            Self::AddSnapshot { bundle_id } => {
                let mut h = open(&store, bundle_id)?;
                let tmp = tempfile::tempdir()?;
                let snap_path = tmp.path().join("desktop.json");
                write_desktop_snapshot(&snap_path, &cfg.output.capture_dir)?;
                let item = h.add_snapshot(&snap_path)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "item_id": item.id,
                    "kind": "desktop_snapshot",
                }));
            }
            Self::Ocr { bundle_id, item } => {
                let backend = OcrBackend::new(cfg.ocr.command.clone(), cfg.ocr.args.clone());
                let mut h = open(&store, bundle_id)?;
                match ocr_item(&mut h, item, &backend)? {
                    Some(ocr_item) => print_json(&serde_json::json!({
                        "bundle_id": h.id(),
                        "item_id": ocr_item.id,
                        "kind": "ocr",
                        "image_item_id": item,
                    })),
                    None => print_json(&serde_json::json!({
                        "bundle_id": h.id(),
                        "image_item_id": item,
                        "warning": "ocr_failed",
                    })),
                }
            }
            Self::Handoff {
                bundle_id,
                target,
                status,
                transport,
            } => {
                let status = match status {
                    HandoffStatusCli::Queued => HandoffStatus::Queued,
                    HandoffStatusCli::Sent => HandoffStatus::Sent,
                    HandoffStatusCli::Delivered => HandoffStatus::Delivered,
                    HandoffStatusCli::Failed => HandoffStatus::Failed,
                };
                let transport = match transport {
                    TransportCli::LocalProcess => Transport::LocalProcess,
                    TransportCli::Tmux => Transport::Tmux,
                    TransportCli::OqtoApi => Transport::OqtoApi,
                    TransportCli::RemoteAgent => Transport::RemoteAgent,
                    TransportCli::Queue => Transport::Queue,
                };
                let mut h = open(&store, bundle_id)?;
                let record = h.add_handoff(target, transport, status)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "target_id": record.target_id,
                    "transport": format!("{:?}", record.transport),
                    "status": format!("{:?}", record.status),
                }));
            }
            Self::Export {
                bundle_id,
                format: _,
            } => {
                let mut h = open(&store, bundle_id)?;
                let archive = export_ctx(&mut h)?;
                print_json(&serde_json::json!({
                    "bundle_id": h.id(),
                    "archive_path": archive.display().to_string(),
                    "format": "ctx-archive",
                }));
            }
            Self::Show { bundle_id } => {
                let manifest = store.load(bundle_id)?;
                let mut value = serde_json::to_value(&manifest)?;
                if let serde_json::Value::Object(ref mut map) = value {
                    map.insert(
                        "bundle_path".to_string(),
                        serde_json::json!(store.bundle_dir(bundle_id).display().to_string()),
                    );
                    map.insert(
                        "context_md_path".to_string(),
                        serde_json::json!(
                            store.bundle_dir(bundle_id).join("context.md").display().to_string()
                        ),
                    );
                }
                print_json(&value);
            }
            Self::List => {
                let ids = store.list()?;
                let entries: Vec<_> = ids
                    .iter()
                    .map(|id| {
                        let manifest = store.load(id).ok();
                        serde_json::json!({
                            "id": id,
                            "state": manifest.as_ref().map(|m| format!("{:?}", m.state).to_lowercase()),
                            "slug": manifest.as_ref().and_then(|m| m.slug.clone()),
                            "items": manifest.as_ref().map(|m| m.items.len()),
                        })
                    })
                    .collect();
                print_json(&serde_json::json!({
                    "bundle_dir": store.root().display().to_string(),
                    "count": entries.len(),
                    "bundles": entries,
                }));
            }
        }
        Ok(())
    }
}

fn open(store: &BundleStore, id: &str) -> anyhow::Result<BundleStoreMut> {
    Ok(BundleStoreMut::new(
        store.clone(),
        store.load(id).map_err(|_| {
            anyhow::anyhow!("bundle not found: {id} (check `ctx bundle list`)")
        })?.id,
    ))
}

/// Resolve the relative path of an item's primary file within the bundle.
fn item_path(h: &BundleStoreMut, item_id: &str) -> String {
    let manifest = match h.manifest() {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    let Some(item) = manifest.find_item(item_id) else {
        return String::new();
    };
    match &item.body {
        ctx_core::manifest::ItemBody::Image(f) => f.path.clone(),
        _ => String::new(),
    }
}

fn resolve_text(text: Option<&str>, file: Option<&str>) -> anyhow::Result<String> {
    match (text, file) {
        (Some(t), _) => Ok(t.to_string()),
        (None, Some("-")) => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
        (None, Some(path)) => Ok(std::fs::read_to_string(path)?),
        (None, None) => Err(anyhow::anyhow!(
            "provide --text or --file (use `-` for stdin)"
        )),
    }
}

fn parse_rect(s: &str) -> anyhow::Result<Rect> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        anyhow::bail!("rect must be x,y,w,h");
    }
    let nums = parts
        .iter()
        .map(|p| p.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Rect::new(nums[0], nums[1], nums[2], nums[3]))
}

fn emit_bundle(manifest: &ctx_core::manifest::Manifest) {
    print_json(&serde_json::json!({
        "id": manifest.id,
        "state": format!("{:?}", manifest.state).to_lowercase(),
        "slug": manifest.slug,
        "bundle_path": manifest_path_of(manifest),
    }));
}

fn manifest_path_of(manifest: &ctx_core::manifest::Manifest) -> String {
    // Reconstruct the bundle path from the store root is not available here;
    // callers that need the path use `ctx bundle show`. For `create` we emit the
    // id and slug which is sufficient to address the bundle.
    manifest.id.clone()
}

fn print_json(value: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into()));
}

/// Entry point used by the top-level CLI when `ctx bundle ...` is invoked.
pub fn run(cli: &BundleCli, cfg: &AppConfig) -> anyhow::Result<()> {
    let _ = config::default_app_name(); // ensure config plumbing is reachable
    cli.command.run(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rect_valid() {
        let r = parse_rect("10,20,30,40").unwrap();
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 40);
    }

    #[test]
    fn parse_rect_spaces() {
        let r = parse_rect(" 1 , 2 , 3 , 4 ").unwrap();
        assert_eq!(r.x, 1);
        assert_eq!(r.width, 3);
    }

    #[test]
    fn parse_rect_invalid() {
        assert!(parse_rect("1,2,3").is_err());
        assert!(parse_rect("1,2,3,a").is_err());
        assert!(parse_rect("1,2,3,4,5").is_err());
    }

    #[test]
    fn resolve_text_prefers_explicit() {
        assert_eq!(resolve_text(Some("hi"), None).unwrap(), "hi");
        assert!(resolve_text(None, None).is_err());
    }
}
