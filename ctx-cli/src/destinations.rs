//! Destination integrations available to the CLI.
//!
//! ctx-core only defines the neutral [`Destination`] seam; concrete
//! integrations live outside ctx-core. The CLI ships a single `fake`
//! destination used for integration testing and for proving the seam without
//! any product coupling.

use std::fs;
use std::path::PathBuf;

use ctx_core::handoff::{
    Destination, DestinationTarget, HandoffError, HandoffMode, HandoffOutcome, HandoffReceipt,
    HandoffRequest, RemoteRef, TargetHints,
};

/// Resolve a destination by name. Unknown destinations fail explicitly and
/// never fall back to another one.
pub fn resolve(name: &str, data_dir: &std::path::Path) -> anyhow::Result<Box<dyn Destination>> {
    match name {
        "fake" => Ok(Box::new(FakeDestination::new(fake_dir(data_dir)))),
        other => anyhow::bail!("unknown destination '{other}' (available: fake)"),
    }
}

fn fake_dir(data_dir: &std::path::Path) -> PathBuf {
    std::env::var_os("CTX_FAKE_DESTINATION_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("fake-destination"))
}

/// A file-backed fake destination: targets come from `targets.json` in its
/// directory (or a built-in default pair), and every accepted handoff is
/// persisted as `received/<request_id>.json`, making retries idempotent
/// across process invocations and receipts auditable.
pub struct FakeDestination {
    dir: PathBuf,
}

impl FakeDestination {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn targets(&self) -> Result<Vec<DestinationTarget>, HandoffError> {
        let path = self.dir.join("targets.json");
        if !path.exists() {
            return Ok(vec![
                DestinationTarget {
                    id: "fake-alpha".to_string(),
                    title: "Alpha".to_string(),
                    description: Some("built-in fake target".to_string()),
                },
                DestinationTarget {
                    id: "fake-beta".to_string(),
                    title: "Beta".to_string(),
                    description: Some("built-in fake target".to_string()),
                },
            ]);
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| HandoffError::Other(format!("read {}: {e}", path.display())))?;
        serde_json::from_str(&content)
            .map_err(|e| HandoffError::Other(format!("parse {}: {e}", path.display())))
    }

    fn receipt_path(&self, request_id: &str) -> PathBuf {
        let safe: String = request_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join("received").join(format!("{safe}.json"))
    }
}

impl Destination for FakeDestination {
    fn list_targets(
        &self,
        query: Option<&str>,
        _hints: &TargetHints,
    ) -> Result<Vec<DestinationTarget>, HandoffError> {
        let targets = self.targets()?;
        Ok(targets
            .into_iter()
            .filter(|t| {
                query
                    .is_none_or(|q| t.id == q || t.title.to_lowercase().contains(&q.to_lowercase()))
            })
            .collect())
    }

    fn handoff(&self, request: &HandoffRequest) -> Result<HandoffReceipt, HandoffError> {
        let receipt_path = self.receipt_path(&request.request_id);
        if receipt_path.exists() {
            let content = fs::read_to_string(&receipt_path)
                .map_err(|e| HandoffError::Other(format!("read receipt: {e}")))?;
            let stored: StoredHandoff = serde_json::from_str(&content)
                .map_err(|e| HandoffError::Other(format!("parse receipt: {e}")))?;
            return Ok(stored.receipt);
        }

        let target = self
            .targets()?
            .into_iter()
            .find(|t| t.id == request.target_id)
            .ok_or_else(|| HandoffError::TargetUnavailable {
                target_id: request.target_id.clone(),
                reason: "unknown target".to_string(),
            })?;

        let receipt = HandoffReceipt {
            request_id: request.request_id.clone(),
            outcome: match request.mode {
                HandoffMode::Stage => HandoffOutcome::Staged,
                HandoffMode::Send => HandoffOutcome::Sent,
            },
            target,
            remote_refs: vec![RemoteRef {
                label: Some("upload".to_string()),
                reference: format!("fake://received/{}", request.request_id),
            }],
            warnings: Vec::new(),
            rejection_reason: None,
            completed_at: time::OffsetDateTime::now_utc(),
        };

        let stored = StoredHandoff {
            request: request.clone(),
            receipt: receipt.clone(),
        };
        if let Some(parent) = receipt_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| HandoffError::Other(format!("create receipt dir: {e}")))?;
        }
        let payload = serde_json::to_vec_pretty(&stored)
            .map_err(|e| HandoffError::Other(format!("serialize receipt: {e}")))?;
        fs::write(&receipt_path, payload)
            .map_err(|e| HandoffError::Other(format!("write receipt: {e}")))?;
        Ok(receipt)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredHandoff {
    request: HandoffRequest,
    receipt: HandoffReceipt,
}
