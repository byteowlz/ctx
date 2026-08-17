//! Integration tests for the deliberate bundle handoff CLI flow using the
//! built-in fake destination.

use std::process::{Command, Output, Stdio};

use serde_json::Value;

struct Env {
    _temp: tempfile::TempDir,
    base: std::path::PathBuf,
}

impl Env {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path().to_path_buf();
        Self { _temp: temp, base }
    }

    fn fake_dir(&self) -> std::path::PathBuf {
        self.base.join("fake-destination")
    }

    fn ctx(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ctx"))
            .args(args)
            .env("XDG_CONFIG_HOME", self.base.join("config"))
            .env("XDG_DATA_HOME", self.base.join("data"))
            .env("XDG_STATE_HOME", self.base.join("state"))
            .env("CTX_FAKE_DESTINATION_DIR", self.fake_dir())
            .stdin(Stdio::null())
            .output()
            .expect("run ctx")
    }
}

fn json_docs(output: &Output) -> Vec<Value> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::Deserializer::from_str(&stdout)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|e| panic!("parse stdout as JSON: {e}\nstdout: {stdout}"))
}

fn create_bundle(env: &Env) -> String {
    let out = env.ctx(&["bundle", "create", "--slug", "handoff-test"]);
    assert!(out.status.success(), "create failed: {out:?}");
    let docs = json_docs(&out);
    let id = docs[0]["id"].as_str().expect("bundle id").to_string();
    let out = env.ctx(&[
        "bundle",
        "add-text",
        &id,
        "--role",
        "task",
        "--text",
        "self-contained test bundle",
    ]);
    assert!(out.status.success(), "add-text failed: {out:?}");
    id
}

fn received_count(env: &Env) -> usize {
    let dir = env.fake_dir().join("received");
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir).unwrap().count()
}

#[test]
fn bundle_creation_alone_performs_no_handoff() {
    let env = Env::new();
    let _id = create_bundle(&env);
    let out = env.ctx(&["current"]);
    assert!(out.status.success());
    assert!(
        !env.fake_dir().exists(),
        "no destination activity may happen without an explicit send"
    );
}

#[test]
fn send_requires_explicit_confirmation_when_noninteractive() {
    let env = Env::new();
    let id = create_bundle(&env);
    let out = env.ctx(&[
        "bundle",
        "send",
        &id,
        "--destination",
        "fake",
        "--target",
        "fake-alpha",
    ]);
    assert!(!out.status.success(), "must fail without --yes: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--yes"), "stderr: {stderr}");
    assert_eq!(received_count(&env), 0);
}

#[test]
fn send_with_confirmation_prints_receipt_and_retry_is_idempotent() {
    let env = Env::new();
    let id = create_bundle(&env);

    let args = [
        "bundle",
        "send",
        &id,
        "--destination",
        "fake",
        "--target",
        "fake-alpha",
        "--mode",
        "send",
        "--instruction",
        "process this bundle",
        "--request-id",
        "req-idempotent-1",
        "--yes",
    ];
    let out = env.ctx(&args);
    assert!(out.status.success(), "send failed: {out:?}");
    let docs = json_docs(&out);

    let preview = &docs[0]["preview"];
    assert_eq!(preview["request"]["mode"], "send");
    assert_eq!(preview["request"]["bundle"]["bundle_id"], id.as_str());
    assert!(preview["request"]["bundle"]["archive_hash"]["value"].is_string());

    let receipt = &docs[1]["receipt"];
    assert_eq!(receipt["outcome"], "sent");
    assert_eq!(receipt["request_id"], "req-idempotent-1");
    assert_eq!(receipt["target"]["id"], "fake-alpha");
    assert!(receipt["remote_refs"][0]["reference"].is_string());
    assert_eq!(received_count(&env), 1);

    // Retrying with the same idempotency identity does not duplicate.
    let out = env.ctx(&args);
    assert!(out.status.success(), "retry failed: {out:?}");
    let docs = json_docs(&out);
    let retry_receipt = &docs[1]["receipt"];
    assert_eq!(retry_receipt["request_id"], "req-idempotent-1");
    assert_eq!(retry_receipt["completed_at"], receipt["completed_at"]);
    assert_eq!(received_count(&env), 1);
}

#[test]
fn stage_mode_returns_staged_receipt() {
    let env = Env::new();
    let id = create_bundle(&env);
    let out = env.ctx(&[
        "bundle",
        "send",
        &id,
        "--destination",
        "fake",
        "--target",
        "fake-beta",
        "--mode",
        "stage",
        "--yes",
    ]);
    assert!(out.status.success(), "stage failed: {out:?}");
    let docs = json_docs(&out);
    assert_eq!(docs[1]["receipt"]["outcome"], "staged");
}

#[test]
fn ambiguous_target_presents_choices_and_fails() {
    let env = Env::new();
    let id = create_bundle(&env);
    let out = env.ctx(&["bundle", "send", &id, "--destination", "fake", "--yes"]);
    assert!(!out.status.success(), "ambiguous send must fail: {out:?}");
    let docs = json_docs(&out);
    assert_eq!(docs[0]["error"], "ambiguous_target");
    assert_eq!(docs[0]["choices"].as_array().unwrap().len(), 2);
    assert_eq!(received_count(&env), 0);
}

#[test]
fn unknown_target_fails_without_fallback_and_preserves_bundle() {
    let env = Env::new();
    let id = create_bundle(&env);
    let out = env.ctx(&[
        "bundle",
        "send",
        &id,
        "--destination",
        "fake",
        "--target",
        "does-not-exist",
        "--yes",
    ]);
    assert!(!out.status.success(), "must fail: {out:?}");
    let docs = json_docs(&out);
    let failure = docs
        .iter()
        .find(|d| d["error"] == "handoff_failed")
        .expect("failure doc");
    assert_eq!(failure["bundle_preserved"], true);
    assert_eq!(received_count(&env), 0);

    // Local bundle still loads.
    let out = env.ctx(&["bundle", "show", &id]);
    assert!(out.status.success(), "bundle must survive: {out:?}");
}

#[test]
fn targets_lists_with_query_filter_and_hints() {
    let env = Env::new();
    // Custom target set for the fake destination.
    let dir = env.fake_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("targets.json"),
        r#"[{"id":"proj-x","title":"Project X"},{"id":"proj-y","title":"Project Y"}]"#,
    )
    .unwrap();

    let out = env.ctx(&["bundle", "targets", "--destination", "fake"]);
    assert!(out.status.success(), "targets failed: {out:?}");
    let docs = json_docs(&out);
    assert_eq!(docs[0]["targets"].as_array().unwrap().len(), 2);

    let out = env.ctx(&["bundle", "targets", "--destination", "fake", "--query", "x"]);
    let docs = json_docs(&out);
    let targets = docs[0]["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0]["id"], "proj-x");
}

#[test]
fn unknown_destination_fails_explicitly() {
    let env = Env::new();
    let id = create_bundle(&env);
    let out = env.ctx(&[
        "bundle",
        "send",
        &id,
        "--destination",
        "oqto",
        "--target",
        "t",
        "--yes",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown destination"), "stderr: {stderr}");
}
