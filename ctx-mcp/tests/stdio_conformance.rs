//! MCP conformance tests: start ctx-mcp over stdio as a generic MCP client
//! would, with no target-specific code, and exercise discovery, listing,
//! reading, error handling, and traversal safety.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};

use serde_json::{Value, json};

use ctx_core::current::{CurrentContextReport, report_current_context};
use ctx_core::manifest::{
    FileFields, FileStorage, ImageFields, ImageProvenance, ImageRole, Item, ItemBody, Producer,
    TextRole,
};
use ctx_core::store::BundleStore;

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path().to_path_buf();
        Self { _temp: temp, base }
    }

    fn state_file(&self) -> PathBuf {
        self.base.join("state/ctx/current-context.json")
    }

    fn bundle_store(&self) -> BundleStore {
        BundleStore::new(self.base.join("data/ctx/bundles")).expect("store")
    }

    fn report_context(&self, cwd: &str) {
        report_current_context(
            &self.state_file(),
            CurrentContextReport {
                source: Some("test".to_string()),
                cwd: Some(cwd.to_string()),
                ..Default::default()
            },
        )
        .expect("report context");
    }

    fn spawn(&self, extra_env: &[(&str, &str)]) -> McpClient {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx-mcp"));
        cmd.env("XDG_CONFIG_HOME", self.base.join("config"))
            .env("XDG_DATA_HOME", self.base.join("data"))
            .env("XDG_STATE_HOME", self.base.join("state"))
            .env("CTX_MCP_DEVICE_ID", "testdev")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().expect("spawn ctx-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        McpClient {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{msg}").expect("write request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        let response: Value = serde_json::from_str(&line).expect("parse response");
        assert_eq!(response["id"], json!(id), "response correlates to request");
        assert_eq!(response["jsonrpc"], "2.0");
        response
    }

    fn expect_result(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params);
        assert!(
            response.get("error").is_none(),
            "{method} unexpectedly failed: {response}"
        );
        response["result"].clone()
    }

    fn expect_error(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params);
        assert!(
            response.get("error").is_some(),
            "{method} unexpectedly succeeded: {response}"
        );
        response["error"].clone()
    }

    fn initialize(&mut self) -> Value {
        self.expect_result(
            "initialize",
            json!({"protocolVersion": "2026-07-28", "capabilities": {}, "clientInfo": {"name": "generic-test-client", "version": "0"}}),
        )
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn create_bundle(fixture: &Fixture) -> (String, String, String) {
    let store = fixture.bundle_store();
    let (manifest, mut handle) = store
        .create(Some("mcp-test"), Producer::new("ctx"))
        .unwrap();
    let text_item = handle
        .add_text(TextRole::Task, "read me over MCP", None)
        .unwrap();

    let image_rel = "images/pixel.png";
    let image_abs = store.bundle_dir(&manifest.id).join(image_rel);
    std::fs::create_dir_all(image_abs.parent().unwrap()).unwrap();
    std::fs::write(&image_abs, fake_png()).unwrap();
    let image_item = handle
        .add_image_item(
            ImageRole::Screenshot,
            image_rel,
            None,
            ImageProvenance::default(),
            None,
        )
        .unwrap();
    (manifest.id, text_item.id, image_item.id)
}

fn fake_png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend_from_slice(&[0u8; 64]);
    bytes
}

#[test]
fn discovery_reports_version_and_pull_only_capabilities() {
    let fixture = Fixture::new();
    let mut client = fixture.spawn(&[]);

    let init = client.initialize();
    assert_eq!(init["protocolVersion"], "2026-07-28");
    assert_eq!(init["capabilities"]["resources"]["subscribe"], false);
    assert_eq!(init["serverInfo"]["name"], "ctx-mcp");

    let discover = client.expect_result("server/discover", json!({}));
    assert_eq!(discover["serverInfo"]["name"], "ctx-mcp");

    let pong = client.expect_result("ping", json!({}));
    assert_eq!(pong, json!({}));
}

#[test]
fn list_is_deterministic_paginated_and_private_cached() {
    let fixture = Fixture::new();
    fixture.report_context("/tmp/proj");
    let (bundle_id, _, _) = create_bundle(&fixture);
    let mut client = fixture.spawn(&[("CTX_MCP_PAGE_SIZE", "1")]);
    client.initialize();

    let first = client.expect_result("resources/list", json!({}));
    let resources = first["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["uri"], "ctx://devices/testdev/current");
    assert_eq!(
        resources[0]["_meta"]["io.byteowlz.ctx/cache"]["scope"],
        "private"
    );
    let cursor = first["nextCursor"].as_str().unwrap().to_string();

    let second = client.expect_result("resources/list", json!({"cursor": cursor}));
    let resources = second["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["uri"], format!("ctx://bundles/{bundle_id}"));
    assert!(second.get("nextCursor").is_none());

    // Deterministic: same listing again yields the same first page.
    let again = client.expect_result("resources/list", json!({}));
    assert_eq!(
        again["resources"][0]["uri"],
        "ctx://devices/testdev/current"
    );

    // Only current-context and bundle resources exist; no live capture
    // (clipboard/screenshot/selection) is reachable through MCP.
    let templates = client.expect_result("resources/templates/list", json!({}));
    let uris: Vec<&str> = templates["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert_eq!(
        uris,
        vec![
            "ctx://bundles/{bundle_id}",
            "ctx://bundles/{bundle_id}/items/{item_id}"
        ]
    );
}

#[test]
fn read_current_returns_complete_snapshot_and_sequence_increments() {
    let fixture = Fixture::new();
    fixture.report_context("/tmp/first");
    let mut client = fixture.spawn(&[]);
    client.initialize();

    let result = client.expect_result(
        "resources/read",
        json!({"uri": "ctx://devices/testdev/current"}),
    );
    let content = &result["contents"][0];
    assert_eq!(content["mimeType"], "application/json");
    let snapshot: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
    assert_eq!(snapshot["sequence"], 1);
    assert_eq!(snapshot["active"]["cwd"], "/tmp/first");

    // Update the state out-of-band; the consumer re-reads the complete
    // replace-whole snapshot (no event stream reconstruction).
    fixture.report_context("/tmp/second");
    let result = client.expect_result(
        "resources/read",
        json!({"uri": "ctx://devices/testdev/current"}),
    );
    let snapshot: Value =
        serde_json::from_str(result["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(snapshot["sequence"], 2);
    assert_eq!(snapshot["active"]["cwd"], "/tmp/second");

    // Unknown device ids are structured not-found errors.
    let error = client.expect_error(
        "resources/read",
        json!({"uri": "ctx://devices/otherdevice/current"}),
    );
    assert_eq!(error["code"], -32002);
}

#[test]
fn read_bundle_summary_text_and_image_items() {
    let fixture = Fixture::new();
    let (bundle_id, text_id, image_id) = create_bundle(&fixture);
    let mut client = fixture.spawn(&[]);
    client.initialize();

    let summary = client.expect_result(
        "resources/read",
        json!({"uri": format!("ctx://bundles/{bundle_id}")}),
    );
    let body: Value =
        serde_json::from_str(summary["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["manifest"]["id"], bundle_id.as_str());
    assert_eq!(body["manifest"]["producer"]["name"], "ctx");
    let item_uris: Vec<&str> = body["item_resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["uri"].as_str().unwrap())
        .collect();
    assert!(item_uris.contains(&format!("ctx://bundles/{bundle_id}/items/{text_id}").as_str()));

    let text = client.expect_result(
        "resources/read",
        json!({"uri": format!("ctx://bundles/{bundle_id}/items/{text_id}")}),
    );
    assert_eq!(text["contents"][0]["text"], "read me over MCP");
    assert_eq!(text["contents"][0]["mimeType"], "text/plain");

    // Image bytes come back as a typed base64 blob, not forced into text.
    let image = client.expect_result(
        "resources/read",
        json!({"uri": format!("ctx://bundles/{bundle_id}/items/{image_id}")}),
    );
    let content = &image["contents"][0];
    assert_eq!(content["mimeType"], "image/png");
    assert!(content.get("text").is_none());
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(content["blob"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, fake_png());
}

#[test]
fn large_binary_items_are_bounded() {
    let fixture = Fixture::new();
    let (bundle_id, _, image_id) = create_bundle(&fixture);
    let mut client = fixture.spawn(&[("CTX_MCP_MAX_ITEM_BYTES", "8")]);
    client.initialize();

    let error = client.expect_error(
        "resources/read",
        json!({"uri": format!("ctx://bundles/{bundle_id}/items/{image_id}")}),
    );
    assert_eq!(error["code"], -32003);
    assert_eq!(error["data"]["reason"], "too_large");
    assert_eq!(error["data"]["mimeType"], "image/png");
    assert_eq!(error["data"]["limitBytes"], 8);
}

#[test]
fn malformed_manifests_cannot_escape_the_bundle_root() {
    let fixture = Fixture::new();
    let store = fixture.bundle_store();
    let (manifest, mut handle) = store.create(Some("evil"), Producer::new("ctx")).unwrap();

    // Plant a secret outside the bundle root and a manifest that points at it.
    let secret = fixture.base.join("secret.txt");
    std::fs::write(&secret, "do not serve").unwrap();
    let mut m = store.load(&manifest.id).unwrap();
    m.items.push(Item::new(
        "img_evil",
        ItemBody::Image(ImageFields {
            role: ImageRole::Screenshot,
            path: "../../secret.txt".to_string(),
            mime: Some("image/png".to_string()),
            dimensions: None,
            provenance: ImageProvenance::default(),
            ocr_refs: Vec::new(),
            cropped_from: None,
            note: None,
        }),
    ));
    m.items.push(Item::new(
        "img_abs",
        ItemBody::Image(ImageFields {
            role: ImageRole::Screenshot,
            path: secret.display().to_string(),
            mime: Some("image/png".to_string()),
            dimensions: None,
            provenance: ImageProvenance::default(),
            ocr_refs: Vec::new(),
            cropped_from: None,
            note: None,
        }),
    ));
    m.items.push(Item::new(
        "file_ref",
        ItemBody::File(FileFields {
            storage: FileStorage::Referenced {
                path: secret.display().to_string(),
            },
            size_bytes: None,
            mime: None,
            hash: None,
            note: None,
        }),
    ));
    handle.save(&mut m).unwrap();

    let mut client = fixture.spawn(&[]);
    client.initialize();
    for item in ["img_evil", "img_abs", "file_ref"] {
        let error = client.expect_error(
            "resources/read",
            json!({"uri": format!("ctx://bundles/{}/items/{item}", manifest.id)}),
        );
        assert_eq!(error["code"], -32003, "item {item}: {error}");
        let reason = error["data"]["reason"].as_str().unwrap();
        assert!(
            reason.contains("bundle root"),
            "item {item} leaked: {reason}"
        );
    }

    // URI-level traversal is rejected before any file access.
    let error = client.expect_error(
        "resources/read",
        json!({"uri": "ctx://bundles/../secret.txt"}),
    );
    assert_eq!(error["code"], -32602);
}

#[test]
fn missing_resources_return_structured_errors() {
    let fixture = Fixture::new();
    let (bundle_id, _, _) = create_bundle(&fixture);
    let mut client = fixture.spawn(&[]);
    client.initialize();

    let error = client.expect_error("resources/read", json!({"uri": "ctx://bundles/nope"}));
    assert_eq!(error["code"], -32002);
    assert_eq!(error["data"]["reason"], "bundle not found");

    let error = client.expect_error(
        "resources/read",
        json!({"uri": format!("ctx://bundles/{bundle_id}/items/missing_item")}),
    );
    assert_eq!(error["code"], -32002);
}

#[test]
fn subscriptions_are_deferred_with_structured_error() {
    let fixture = Fixture::new();
    let mut client = fixture.spawn(&[]);
    client.initialize();
    for method in ["resources/subscribe", "subscriptions/listen"] {
        let error = client.expect_error(method, json!({"uri": "ctx://devices/testdev/current"}));
        assert_eq!(error["code"], -32601, "{method}");
        assert_eq!(error["data"]["reason"], "subscriptions_deferred");
    }
}
