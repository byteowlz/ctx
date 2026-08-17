//! ctx-mcp: a target-neutral, read-only MCP Resource server for deliberate
//! access to ctx Current Context and durable Bundles.
//!
//! The server is pull-only: listing and reading resources never injects
//! model context, sends messages, invokes tools, or calls an AI provider.
//! The MCP host decides whether a selected resource enters a prompt; ctx
//! supplies resources, provenance, and structured unavailable outcomes only.
//!
//! Transport is line-delimited JSON-RPC 2.0 over stdio (the MCP stdio
//! transport). No network listener exists in this crate; there is no HTTP
//! mode to accidentally enable. Subscriptions are deferred: capability
//! reporting advertises `subscribe: false` and `resources/subscribe` returns
//! a structured error telling consumers to re-read the complete snapshot.
//!
//! Resource URIs (v1, stable):
//! - `ctx://devices/<device-id>/current` - complete Current Context snapshot
//! - `ctx://bundles/<bundle-id>` - Bundle summary (full manifest + item URIs)
//! - `ctx://bundles/<bundle-id>/items/<item-id>` - one Bundle item
//!
//! Local filesystem paths are implementation details and never become
//! resource identity.

use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use serde_json::{Value, json};

use ctx_core::current::read_current_context;
use ctx_core::manifest::{FileStorage, ItemBody, Manifest, TextContent};
use ctx_core::store::BundleStore;

/// MCP protocol revision this server targets.
pub const PROTOCOL_VERSION: &str = "2026-07-28";
/// Older protocol revisions the server also accepts from clients.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-06-18", "2025-03-26"];

/// Default cap on bytes served inline for one item read.
pub const DEFAULT_MAX_ITEM_BYTES: u64 = 4 * 1024 * 1024;
/// Default `resources/list` page size.
pub const DEFAULT_PAGE_SIZE: usize = 50;

const CACHE_META_KEY: &str = "io.byteowlz.ctx/cache";

// JSON-RPC / MCP error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const RESOURCE_NOT_FOUND: i64 = -32002;
const RESOURCE_UNAVAILABLE: i64 = -32003;

/// Server configuration resolved by the caller (binary or test).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Path of the Current Context state file.
    pub state_file: PathBuf,
    /// Root directory of the Bundle store.
    pub bundle_dir: PathBuf,
    /// Stable identifier for this device in resource URIs.
    pub device_id: String,
    /// Maximum bytes served inline for a single item read.
    pub max_item_bytes: u64,
    /// Page size for `resources/list`.
    pub page_size: usize,
}

impl ServerConfig {
    /// Build a config from resolved ctx paths, honoring `CTX_MCP_DEVICE_ID`,
    /// `CTX_MCP_MAX_ITEM_BYTES`, and `CTX_MCP_PAGE_SIZE` env overrides.
    pub fn from_paths(state_file: PathBuf, bundle_dir: PathBuf) -> Self {
        let device_id = std::env::var("CTX_MCP_DEVICE_ID")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(sysinfo::System::host_name)
            .map(|raw| sanitize_id(&raw))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "local".to_string());
        let max_item_bytes = std::env::var("CTX_MCP_MAX_ITEM_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MAX_ITEM_BYTES);
        let page_size = std::env::var("CTX_MCP_PAGE_SIZE")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|&size: &usize| size > 0)
            .unwrap_or(DEFAULT_PAGE_SIZE);
        Self {
            state_file,
            bundle_dir,
            device_id,
            max_item_bytes,
            page_size,
        }
    }
}

fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_string()
}

/// The read-only MCP resource server.
pub struct Server {
    config: ServerConfig,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    /// Handle one line-delimited JSON-RPC message. Returns the serialized
    /// response, or `None` for notifications.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => {
                return Some(
                    error_response(
                        Value::Null,
                        PARSE_ERROR,
                        &format!("parse error: {err}"),
                        None,
                    )
                    .to_string(),
                );
            }
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str);
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let Some(method) = method else {
            return id
                .map(|id| error_response(id, INVALID_REQUEST, "missing method", None).to_string());
        };
        if method.starts_with("notifications/") {
            return None;
        }
        // A request without an id is a notification; never answer it.
        let id = id?;

        let outcome = self.dispatch(method, &params);
        let response = match outcome {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(err) => error_response(id, err.code, &err.message, err.data),
        };
        Some(response.to_string())
    }

    fn dispatch(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" | "server/discover" => Ok(self.discovery_payload(params)),
            "ping" => Ok(json!({})),
            "resources/list" => self.resources_list(params),
            "resources/templates/list" => Ok(self.templates_list()),
            "resources/read" => self.resources_read(params),
            "resources/subscribe" | "resources/unsubscribe" | "subscriptions/listen" => {
                Err(RpcError {
                    code: METHOD_NOT_FOUND,
                    message: "subscriptions are not supported; re-read \
                              ctx://devices/<device-id>/current to obtain the complete snapshot"
                        .to_string(),
                    data: Some(json!({"reason": "subscriptions_deferred"})),
                })
            }
            other => Err(RpcError {
                code: METHOD_NOT_FOUND,
                message: format!("method not found: {other}"),
                data: None,
            }),
        }
    }

    fn discovery_payload(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION);
        let negotiated = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
            requested
        } else {
            PROTOCOL_VERSION
        };
        json!({
            "protocolVersion": negotiated,
            "capabilities": {
                "resources": {"subscribe": false, "listChanged": false}
            },
            "serverInfo": {
                "name": "ctx-mcp",
                "title": "ctx read-only context resources",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": "Read-only resources for ctx Current Context and Bundles. \
                Reading a resource never injects it into a prompt; the host decides \
                what enters model context. Subscriptions are unsupported: re-read \
                the current-context resource for the complete replace-whole state.",
        })
    }

    fn current_uri(&self) -> String {
        format!("ctx://devices/{}/current", self.config.device_id)
    }

    fn cache_meta(&self) -> Value {
        json!({CACHE_META_KEY: {"scope": "private", "ttlSeconds": 5}})
    }

    fn resources_list(&self, params: &Value) -> Result<Value, RpcError> {
        let cursor = match params.get("cursor") {
            None | Some(Value::Null) => 0usize,
            Some(Value::String(raw)) => raw.parse().map_err(|_| RpcError {
                code: INVALID_PARAMS,
                message: format!("invalid cursor: {raw}"),
                data: None,
            })?,
            Some(other) => {
                return Err(RpcError {
                    code: INVALID_PARAMS,
                    message: format!("invalid cursor: {other}"),
                    data: None,
                });
            }
        };

        let mut resources = vec![json!({
            "uri": self.current_uri(),
            "name": "current-context",
            "title": "Current Context",
            "description": "Complete lightweight Current Context snapshot \
                (replace-whole state with sequence and updated time)",
            "mimeType": "application/json",
            "annotations": {"audience": ["assistant", "user"]},
            "_meta": self.cache_meta(),
        })];

        let store = self.store()?;
        let mut ids = store.list().map_err(internal_error)?;
        ids.sort();
        for id in ids {
            let (title, description) = match store.load(&id) {
                Ok(manifest) => (
                    manifest.slug.clone().unwrap_or_else(|| id.clone()),
                    format!(
                        "Context bundle ({} items, state {:?})",
                        manifest.items.len(),
                        manifest.state
                    )
                    .to_lowercase(),
                ),
                Err(_) => (
                    id.clone(),
                    "context bundle (manifest unreadable)".to_string(),
                ),
            };
            resources.push(json!({
                "uri": format!("ctx://bundles/{id}"),
                "name": id,
                "title": title,
                "description": description,
                "mimeType": "application/json",
                "annotations": {"audience": ["assistant", "user"]},
                "_meta": self.cache_meta(),
            }));
        }

        let page: Vec<Value> = resources
            .iter()
            .skip(cursor)
            .take(self.config.page_size)
            .cloned()
            .collect();
        let next = cursor + page.len();
        let mut result = json!({"resources": page, "_meta": self.cache_meta()});
        if next < resources.len() {
            result["nextCursor"] = json!(next.to_string());
        }
        Ok(result)
    }

    fn templates_list(&self) -> Value {
        json!({
            "resourceTemplates": [
                {
                    "uriTemplate": "ctx://bundles/{bundle_id}",
                    "name": "bundle-summary",
                    "title": "Bundle summary",
                    "description": "Full manifest of one context bundle plus item resource URIs",
                    "mimeType": "application/json",
                },
                {
                    "uriTemplate": "ctx://bundles/{bundle_id}/items/{item_id}",
                    "name": "bundle-item",
                    "title": "Bundle item",
                    "description": "One bundle item's content with its provenance preserved in the summary",
                }
            ]
        })
    }

    fn resources_read(&self, params: &Value) -> Result<Value, RpcError> {
        let uri = params.get("uri").and_then(Value::as_str).ok_or(RpcError {
            code: INVALID_PARAMS,
            message: "missing uri".to_string(),
            data: None,
        })?;
        match parse_uri(uri)? {
            Resource::Current { device_id } => {
                if device_id != self.config.device_id {
                    return Err(not_found(uri, "unknown device"));
                }
                let context =
                    read_current_context(&self.config.state_file).map_err(internal_error)?;
                let text = serde_json::to_string_pretty(&context).map_err(internal_error)?;
                Ok(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": text,
                        "_meta": self.cache_meta(),
                    }]
                }))
            }
            Resource::Bundle { bundle_id } => {
                let manifest = self.load_bundle(uri, &bundle_id)?;
                let items: Vec<Value> = manifest
                    .items
                    .iter()
                    .map(|item| {
                        json!({
                            "id": item.id,
                            "kind": item.kind(),
                            "uri": format!("ctx://bundles/{bundle_id}/items/{}", item.id),
                        })
                    })
                    .collect();
                let summary = json!({
                    "uri": uri,
                    "manifest": manifest,
                    "item_resources": items,
                });
                Ok(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&summary).map_err(internal_error)?,
                        "_meta": self.cache_meta(),
                    }]
                }))
            }
            Resource::Item { bundle_id, item_id } => {
                let manifest = self.load_bundle(uri, &bundle_id)?;
                let item = manifest
                    .find_item(&item_id)
                    .ok_or_else(|| not_found(uri, "item not found in bundle"))?;
                let bundle_dir = self.store()?.bundle_dir(&bundle_id);
                self.read_item(uri, &bundle_dir, &item.body)
            }
        }
    }

    fn store(&self) -> Result<BundleStore, RpcError> {
        BundleStore::new(&self.config.bundle_dir).map_err(internal_error)
    }

    fn load_bundle(&self, uri: &str, bundle_id: &str) -> Result<Manifest, RpcError> {
        self.store()?
            .load(bundle_id)
            .map_err(|_| not_found(uri, "bundle not found"))
    }

    fn read_item(&self, uri: &str, bundle_dir: &Path, body: &ItemBody) -> Result<Value, RpcError> {
        match body {
            ItemBody::Text(fields) => match &fields.content {
                TextContent::Inline { text } => Ok(text_contents(uri, "text/plain", text.clone())),
                TextContent::File { path } => {
                    let abs = safe_join(uri, bundle_dir, path)?;
                    let text = std::fs::read_to_string(&abs)
                        .map_err(|e| unavailable(uri, &format!("text file unreadable: {e}")))?;
                    Ok(text_contents(uri, "text/plain", text))
                }
            },
            ItemBody::Url(fields) => Ok(text_contents(uri, "text/uri-list", fields.url.clone())),
            ItemBody::DesktopSnapshot(fields) => {
                let abs = safe_join(uri, bundle_dir, &fields.path)?;
                let text = std::fs::read_to_string(&abs)
                    .map_err(|e| unavailable(uri, &format!("snapshot unreadable: {e}")))?;
                Ok(text_contents(uri, "application/json", text))
            }
            ItemBody::Image(fields) => self.blob_contents(
                uri,
                bundle_dir,
                &fields.path,
                fields.mime.as_deref().unwrap_or("application/octet-stream"),
            ),
            ItemBody::File(fields) => match &fields.storage {
                FileStorage::Copied { bundle_path, .. } => self.blob_contents(
                    uri,
                    bundle_dir,
                    bundle_path,
                    fields.mime.as_deref().unwrap_or("application/octet-stream"),
                ),
                FileStorage::Referenced { .. } => Err(unavailable(
                    uri,
                    "item references a file outside the bundle root; it is not served over MCP",
                )),
            },
        }
    }

    fn blob_contents(
        &self,
        uri: &str,
        bundle_dir: &Path,
        rel_path: &str,
        mime: &str,
    ) -> Result<Value, RpcError> {
        let abs = safe_join(uri, bundle_dir, rel_path)?;
        let size = std::fs::metadata(&abs)
            .map_err(|e| unavailable(uri, &format!("file unreadable: {e}")))?
            .len();
        if size > self.config.max_item_bytes {
            return Err(RpcError {
                code: RESOURCE_UNAVAILABLE,
                message: format!(
                    "item exceeds inline read limit ({size} > {} bytes); \
                     raise CTX_MCP_MAX_ITEM_BYTES or use the local bundle export",
                    self.config.max_item_bytes
                ),
                data: Some(json!({
                    "uri": uri,
                    "reason": "too_large",
                    "sizeBytes": size,
                    "limitBytes": self.config.max_item_bytes,
                    "mimeType": mime,
                })),
            });
        }
        let bytes =
            std::fs::read(&abs).map_err(|e| unavailable(uri, &format!("file unreadable: {e}")))?;
        Ok(json!({
            "contents": [{
                "uri": uri,
                "mimeType": mime,
                "blob": base64::engine::general_purpose::STANDARD.encode(bytes),
                "_meta": {
                    CACHE_META_KEY: {"scope": "private", "ttlSeconds": 5},
                    "io.byteowlz.ctx/sizeBytes": size,
                },
            }]
        }))
    }
}

enum Resource {
    Current { device_id: String },
    Bundle { bundle_id: String },
    Item { bundle_id: String, item_id: String },
}

fn parse_uri(uri: &str) -> Result<Resource, RpcError> {
    let invalid = || RpcError {
        code: INVALID_PARAMS,
        message: format!("unsupported resource uri: {uri}"),
        data: None,
    };
    let rest = uri.strip_prefix("ctx://").ok_or_else(invalid)?;
    let segments: Vec<&str> = rest.split('/').collect();
    for segment in &segments {
        if !is_safe_segment(segment) {
            return Err(invalid());
        }
    }
    match segments.as_slice() {
        ["devices", device_id, "current"] => Ok(Resource::Current {
            device_id: (*device_id).to_string(),
        }),
        ["bundles", bundle_id] => Ok(Resource::Bundle {
            bundle_id: (*bundle_id).to_string(),
        }),
        ["bundles", bundle_id, "items", item_id] => Ok(Resource::Item {
            bundle_id: (*bundle_id).to_string(),
            item_id: (*item_id).to_string(),
        }),
        _ => Err(invalid()),
    }
}

fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Join a manifest-relative path onto the bundle directory, rejecting any
/// path that could escape the bundle root (absolute paths, `..`, prefixes).
fn safe_join(uri: &str, bundle_dir: &Path, rel: &str) -> Result<PathBuf, RpcError> {
    let rel_path = Path::new(rel);
    let escapes = rel_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir));
    if escapes || rel_path.is_absolute() {
        return Err(unavailable(
            uri,
            "bundle manifest path escapes the bundle root",
        ));
    }
    Ok(bundle_dir.join(rel_path))
}

fn text_contents(uri: &str, mime: &str, text: String) -> Value {
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": mime,
            "text": text,
            "_meta": {CACHE_META_KEY: {"scope": "private", "ttlSeconds": 5}},
        }]
    })
}

struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

fn not_found(uri: &str, reason: &str) -> RpcError {
    RpcError {
        code: RESOURCE_NOT_FOUND,
        message: format!("resource not found: {uri} ({reason})"),
        data: Some(json!({"uri": uri, "reason": reason})),
    }
}

fn unavailable(uri: &str, reason: &str) -> RpcError {
    RpcError {
        code: RESOURCE_UNAVAILABLE,
        message: format!("resource unavailable: {uri} ({reason})"),
        data: Some(json!({"uri": uri, "reason": reason})),
    }
}

fn internal_error(err: impl std::fmt::Display) -> RpcError {
    RpcError {
        code: -32603,
        message: err.to_string(),
        data: None,
    }
}

fn error_response(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code": code, "message": message});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": error})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uri_accepts_v1_shapes() {
        assert!(matches!(
            parse_uri("ctx://devices/host-1/current"),
            Ok(Resource::Current { .. })
        ));
        assert!(matches!(
            parse_uri("ctx://bundles/2026-01-01T00-00-00Z_abc123"),
            Ok(Resource::Bundle { .. })
        ));
        assert!(matches!(
            parse_uri("ctx://bundles/b1/items/txt_1"),
            Ok(Resource::Item { .. })
        ));
    }

    #[test]
    fn parse_uri_rejects_traversal_and_junk() {
        for uri in [
            "ctx://bundles/../etc",
            "ctx://bundles/a%2Fb",
            "ctx://bundles/a/b/c/d/e",
            "file:///etc/passwd",
            "ctx://bundles/",
            "ctx://bundles/a b",
            "ctx://devices/./current",
        ] {
            assert!(parse_uri(uri).is_err(), "should reject {uri}");
        }
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let dir = Path::new("/tmp/bundle");
        assert!(safe_join("u", dir, "../outside.txt").is_err());
        assert!(safe_join("u", dir, "/etc/passwd").is_err());
        assert!(safe_join("u", dir, "images/../../x").is_err());
        assert!(safe_join("u", dir, "images/ok.png").is_ok());
    }

    #[test]
    fn sanitize_id_strips_unsafe() {
        assert_eq!(sanitize_id("my host!"), "my-host");
        assert_eq!(sanitize_id("dev.example-1"), "dev.example-1");
    }
}
