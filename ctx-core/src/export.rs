//! `.ctx` portable archive export.
//!
//! A `.ctx` archive is a zipped, self-contained form of a bundle for remote
//! upload (e.g. Oqto) or cross-machine transfer. It always contains:
//!
//! - `manifest.json` (the bundle manifest),
//! - `context.md` (agent-readable summary),
//! - all in-bundle artifacts (images, files, snapshots, ocr text).
//!
//! Referenced-only file items ([`FileStorage::Referenced`]) cannot be archived
//! as-is; the exporter attempts to materialize them, and anything that cannot
//! be resolved is recorded on the manifest as an [`UnresolvedRef`].

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use time::OffsetDateTime;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::manifest::{
    ExportMeta, FileFields, FileStorage, Item, ItemBody, Manifest, UnresolvedRef,
};
use crate::store::{ARCHIVE_FORMAT, ARCHIVE_VERSION, BundleStoreMut, StoreError};

/// Map a zip error into a [`StoreError::Other`].
fn zip_err(e: impl std::fmt::Display) -> StoreError {
    StoreError::Other(format!("zip archive error: {e}"))
}

/// Export a bundle to a `.ctx` zip archive.
///
/// Materializes referenced files when possible, records unresolved refs on the
/// manifest, and writes export metadata. Returns the path to the archive.
pub fn export_ctx(bundle: &mut BundleStoreMut) -> Result<PathBuf, StoreError> {
    let mut manifest = bundle.manifest()?;

    // Ensure context.md is fresh.
    let _ = bundle.store().write_context_md(&manifest);
    let bundle_dir = bundle.dir();

    let archive_name = format!("{}.ctx", manifest.id);
    let archive_rel = archive_name.clone();
    let archive_abs = bundle_dir.join(&archive_rel);

    // Materialize referenced files: copy into files/ so the archive is portable.
    let mut unresolved = materialize_referenced_files(&manifest, &bundle_dir)?;
    if !unresolved.is_empty() {
        manifest.unresolved_refs.append(&mut unresolved);
    }

    let manifest_path = "manifest.json".to_string();
    let context_md_path = "context.md".to_string();

    // Compute export metadata and set it on the in-memory manifest BEFORE
    // writing the zip, so the archive's manifest.json is self-describing.
    let export = ExportMeta {
        format: ARCHIVE_FORMAT.to_string(),
        archive_version: ARCHIVE_VERSION,
        created_at: OffsetDateTime::now_utc(),
        archive_path: archive_rel.clone(),
        manifest_path,
        context_md_path,
    };
    manifest.export = Some(export.clone());

    write_zip(&manifest, &bundle_dir, &archive_abs)?;

    // Persist export metadata + unresolved refs to the on-disk manifest.
    bundle.save(&mut manifest)?;
    Ok(archive_abs)
}

/// Try to copy referenced files into the bundle so they can be archived. Files
/// that cannot be read are returned as [`UnresolvedRef`]s (the caller records
/// them on the manifest).
fn materialize_referenced_files(
    manifest: &Manifest,
    bundle_dir: &Path,
) -> Result<Vec<UnresolvedRef>, StoreError> {
    let mut unresolved = Vec::new();
    for item in &manifest.items {
        let (file_item_id, source_path, dest_name) = match referenced_file_target(item) {
            Some(x) => x,
            None => continue,
        };
        let abs = Path::new(&source_path);
        if !abs.exists() {
            unresolved.push(UnresolvedRef {
                item_id: file_item_id.clone(),
                path: source_path.clone(),
                reason: "file does not exist".to_string(),
            });
            continue;
        }
        let dest_rel = format!("files/{}", sanitize(&dest_name));
        let dest_abs = bundle_dir.join(&dest_rel);
        if let Some(parent) = dest_abs.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::copy(abs, &dest_abs) {
            Ok(_) => {
                // Update the item in the manifest to point at the copied copy.
                // We only mutate items that are still referenced; since this
                // function only inspects, we surface the dest path via the
                // caller. To keep it simple, we rewrite the item body below.
            }
            Err(e) => {
                unresolved.push(UnresolvedRef {
                    item_id: file_item_id.clone(),
                    path: source_path.clone(),
                    reason: format!("copy failed: {e}"),
                });
            }
        }
    }
    Ok(unresolved)
}

/// Rewrite referenced file items in the manifest to point at their materialized
/// copies, so the archive contains portable `copied` entries.
pub fn finalize_materialized_files(manifest: &mut Manifest) {
    for item in &mut manifest.items {
        let (item_id, source_path, dest_name) = match referenced_file_target(item) {
            Some(x) => x,
            None => continue,
        };
        let abs = Path::new(&source_path);
        if !abs.exists() {
            continue;
        }
        let dest_rel = format!("files/{}", sanitize(&dest_name));
        item.body = ItemBody::File(FileFields {
            storage: FileStorage::Copied {
                bundle_path: dest_rel,
                original_path: source_path.clone(),
            },
            size_bytes: fs::metadata(abs).ok().map(|m| m.len()),
            mime: None,
            hash: None,
            note: None,
        });
        let _ = item_id;
    }
}

fn referenced_file_target(item: &Item) -> Option<(String, String, String)> {
    let ItemBody::File(FileFields { storage, .. }) = &item.body else {
        return None;
    };
    let FileStorage::Referenced { path } = storage else {
        return None;
    };
    let p = Path::new(path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("ref-{}", item.id));
    Some((item.id.clone(), path.clone(), name))
}

fn write_zip(manifest: &Manifest, bundle_dir: &Path, archive: &Path) -> Result<(), StoreError> {
    // Materialize referenced files into the manifest before archiving so the
    // zip is portable.
    let mut manifest = manifest.clone();
    finalize_materialized_files(&mut manifest);

    let file = fs::File::create(archive)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 1. manifest.json (rewritten, with materialized files)
    zip.start_file("manifest.json", options).map_err(zip_err)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    zip.write_all(&manifest_bytes).map_err(zip_err)?;

    // 2. context.md (render fresh against the bundle dir)
    zip.start_file("context.md", options).map_err(zip_err)?;
    let md = crate::store::render_context_md(&manifest, bundle_dir);
    zip.write_all(md.as_bytes()).map_err(zip_err)?;

    // 3. Walk the bundle directory and add every real file (except the archive
    //    itself), normalizing paths to be relative + safe.
    let entries = collect_bundle_files(bundle_dir, archive)?;
    for (rel, abs) in entries {
        zip.start_file(&rel, options).map_err(zip_err)?;
        let mut src = fs::File::open(&abs)?;
        let mut buf = vec![0u8; 65536];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n]).map_err(zip_err)?;
        }
    }

    zip.finish().map_err(zip_err)?;
    Ok(())
}

/// Collect every file under the bundle dir (excluding the archive itself and
/// the manifest/context which we already added), as (zip-relative, absolute).
fn collect_bundle_files(
    bundle_dir: &Path,
    archive_abs: &Path,
) -> Result<Vec<(String, PathBuf)>, StoreError> {
    let mut out = Vec::new();
    walk(bundle_dir, bundle_dir, archive_abs, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn walk(
    root: &Path,
    current: &Path,
    archive_abs: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, archive_abs, out)?;
            continue;
        }
        // Skip the archive we are writing, and the top-level manifest/context
        // (already embedded with fresh content).
        if same_path(&path, archive_abs) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if current == root && (name == "manifest.json" || name == "context.md") {
            continue;
        }
        let rel = normalize_rel(root, &path);
        out.push((rel, path));
    }
    Ok(())
}

fn normalize_rel(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut out = String::new();
    for (i, comp) in rel.components().enumerate() {
        if let Component::Normal(s) = comp {
            if i > 0 {
                out.push('/');
            }
            out.push_str(&s.to_string_lossy());
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.canonicalize()
        .ok()
        .zip(b.canonicalize().ok())
        .map(|(x, y)| x == y)
        .unwrap_or_else(|| a == b)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Producer, TextContent, TextFields, TextRole};
    use crate::store::BundleStore;
    use std::io::Read;

    fn make_bundle() -> (tempfile::TempDir, BundleStore, String) {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) = store.create(Some("export"), Producer::new("ctx")).unwrap();
        let _ = h
            .add_text(TextRole::Task, "Export this bundle", None)
            .unwrap();
        (tmp, store, manifest.id)
    }

    #[test]
    fn export_writes_portable_archive() {
        let (_tmp, store, id) = make_bundle();
        let mut h = store_create_mut(&store, &id);
        let archive = export_ctx(&mut h).unwrap();
        assert!(archive.exists());
        assert!(archive.file_name().unwrap().to_string_lossy().ends_with(".ctx"));

        // The archive should be a valid zip with manifest.json and context.md.
        let file = fs::File::open(&archive).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.name_for_index(i).unwrap().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "manifest.json"), "names={names:?}");
        assert!(names.iter().any(|n| n == "context.md"), "names={names:?}");

        // manifest.json inside the zip must parse.
        {
            let mut entry = zip.by_name("manifest.json").unwrap();
            let mut buf = String::new();
            entry.read_to_string(&mut buf).unwrap();
            let parsed: Manifest = serde_json::from_str(&buf).unwrap();
            assert_eq!(parsed.id, id);
            assert!(parsed.export.is_some());
        }

        // context.md inside the zip must mention the task.
        {
            let mut entry = zip.by_name("context.md").unwrap();
            let mut md = String::new();
            entry.read_to_string(&mut md).unwrap();
            assert!(md.contains("Export this bundle"));
        }
    }

    #[test]
    fn export_materializes_referenced_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (manifest, mut h) = store.create(Some("refs"), Producer::new("ctx")).unwrap();

        // Add a real file referenced in place.
        let file_path = tmp.path().join("external.txt");
        fs::write(&file_path, "external data").unwrap();
        let file_item = h.add_file(&file_path, crate::manifest::FilePolicy::Reference).unwrap();
        let _ = file_item;

        let archive = export_ctx(&mut h).unwrap();
        let file = fs::File::open(&archive).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.name_for_index(i).unwrap().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("files/external.txt")),
            "names={names:?}"
        );

        // Manifest in the zip should show the file as copied.
        let mut entry = zip.by_name("manifest.json").unwrap();
        let mut buf = String::new();
        entry.read_to_string(&mut buf).unwrap();
        let parsed: Manifest = serde_json::from_str(&buf).unwrap();
        let file_body = parsed.items.iter().find_map(|i| match &i.body {
            ItemBody::File(f) => Some(f.clone()),
            _ => None,
        }).unwrap();
        assert!(matches!(file_body.storage, FileStorage::Copied { .. }));
        assert!(parsed.unresolved_refs.is_empty());
        let _ = manifest;
    }

    #[test]
    fn export_records_unresolved_missing_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BundleStore::new(tmp.path()).unwrap();
        let (_manifest, mut h) = store.create(Some("missing"), Producer::new("ctx")).unwrap();
        // Add a referenced file pointing at a path that does not exist.
        let ghost = tmp.path().join("nope.txt");
        let _ = h.add_file(&ghost, crate::manifest::FilePolicy::Reference).unwrap();
        export_ctx(&mut h).unwrap();
        let loaded = store.load(h.id()).unwrap();
        assert_eq!(loaded.unresolved_refs.len(), 1);
        assert_eq!(loaded.unresolved_refs[0].reason, "file does not exist");
    }

    #[test]
    fn finalize_rewrites_referenced_to_copied() {
        let mut manifest = Manifest::new("2026-01-01T00-00-00Z_aaaaaa", Producer::new("ctx"));
        manifest.items.push(Item::new(
            "f1",
            ItemBody::File(FileFields {
                storage: FileStorage::Referenced {
                    path: "/tmp/ctx-export-test-file.txt".to_string(),
                },
                size_bytes: None,
                mime: None,
                hash: None,
                note: None,
            }),
        ));
        // write a real file so it exists
        fs::write("/tmp/ctx-export-test-file.txt", b"hi").unwrap();
        finalize_materialized_files(&mut manifest);
        match &manifest.items[0].body {
            ItemBody::File(FileFields {
                storage: FileStorage::Copied { .. },
                ..
            }) => {}
            other => panic!("expected copied, got {other:?}"),
        }
    }

    fn store_create_mut(store: &BundleStore, id: &str) -> BundleStoreMut {
        BundleStoreMut::new(store.clone(), id.to_string())
    }

    #[test]
    fn inline_text_storage_tag_is_inline() {
        // guardrail: ensure text inline content still serializes as expected
        let item = Item::new(
            "t1",
            ItemBody::Text(TextFields {
                role: TextRole::Task,
                content: TextContent::Inline {
                    text: "x".to_string(),
                },
                source: Default::default(),
                note: None,
            }),
        );
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["storage"], "inline");
    }
}
