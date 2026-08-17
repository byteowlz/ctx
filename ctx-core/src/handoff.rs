//! Neutral, deliberate Bundle handoff to external destinations.
//!
//! ctx owns capture, Bundle review/export, and these neutral handoff values.
//! Each destination integration owns authentication, target discovery,
//! upload, and prompt/message semantics. Target identifiers and remote
//! references are opaque strings; no target-domain concepts (accounts,
//! sessions, work directories, runners) enter ctx-core.
//!
//! A handoff only ever happens from an explicit user action on a reviewed
//! Bundle. Nothing in this module performs network, model, or prompt
//! activity by itself; it is a value vocabulary plus one small seam
//! ([`Destination`]) that integrations implement out-of-tree.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::current::CurrentContext;
use crate::manifest::FileHash;

/// How the destination should treat a handoff, when it supports the mode.
///
/// Unsupported modes must fail explicitly with
/// [`HandoffError::UnsupportedMode`]; they are never silently downgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffMode {
    /// Make the Bundle available at the target without submitting it.
    Stage,
    /// Deliver the Bundle (and optional instruction) to the target.
    Send,
}

impl std::fmt::Display for HandoffMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stage => write!(f, "stage"),
            Self::Send => write!(f, "send"),
        }
    }
}

/// A destination-side target a Bundle can be handed to.
///
/// The `id` is opaque to ctx; only the destination integration can interpret
/// it. `title`/`description` exist purely for human review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestinationTarget {
    /// Opaque identifier or URI, interpreted only by the destination.
    pub id: String,
    /// Human-readable name shown during review.
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The exact Bundle content a handoff refers to.
///
/// Pinning the exported archive hash makes retries and receipts verifiable
/// without trusting mutable local state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundlePin {
    /// Stable bundle id.
    pub bundle_id: String,
    /// Hash of the exported `.ctx` archive, when exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_hash: Option<FileHash>,
    /// Local path of the exported `.ctx` archive, when exported. Local paths
    /// are transport hints for the integration, never remote identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_path: Option<String>,
}

/// Who initiated the handoff, for audit trails on both sides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffProvenance {
    /// Producing tool, e.g. `ctx`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    /// Producer version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
    /// Originating host name, if the user chose to include it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// A deliberate request to stage or send a reviewed Bundle to one target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffRequest {
    /// Idempotency identity: retrying with the same id must not create a
    /// duplicate handoff at the destination.
    pub request_id: String,
    /// The pinned Bundle content.
    pub bundle: BundlePin,
    /// Optional instruction accompanying the Bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    /// Opaque target id previously discovered via
    /// [`Destination::list_targets`].
    pub target_id: String,
    /// Requested mode.
    pub mode: HandoffMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub provenance: HandoffProvenance,
    /// When the user confirmed the handoff.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Outcome status of a handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffOutcome {
    /// Staged at the target; not submitted.
    Staged,
    /// Delivered/submitted to the target.
    Sent,
    /// The destination refused the handoff.
    Rejected,
}

/// An opaque reference the destination returned for the handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRef {
    /// What the reference points at, e.g. `upload`, `message`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Opaque reference string, interpreted only by the destination.
    pub reference: String,
}

/// The destination's answer to a [`HandoffRequest`], safe to persist and to
/// correlate retries against. Must never contain credentials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffReceipt {
    /// Echoes [`HandoffRequest::request_id`].
    pub request_id: String,
    pub outcome: HandoffOutcome,
    /// The target the Bundle went to, for display.
    pub target: DestinationTarget,
    /// Opaque remote references (upload ids, message ids, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_refs: Vec<RemoteRef>,
    /// Non-fatal destination-side warnings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Why the handoff was rejected, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: OffsetDateTime,
}

/// Neutral target-suggestion hints derived from Current Context.
///
/// Purely advisory: a destination may use them to rank or filter targets but
/// must never silently auto-select an ambiguous target from them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl TargetHints {
    /// Derive hints from the lightweight Current Context snapshot.
    #[must_use]
    pub fn from_current_context(context: &CurrentContext) -> Self {
        Self {
            cwd: context.active.cwd.clone(),
            project: context.active.project.clone(),
            app: context.active.app.clone(),
            url: context.active.url.clone(),
        }
    }
}

/// Structured, legible handoff failures. All of them preserve the local
/// Bundle; a failed handoff never mutates or removes Bundle content.
#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("destination does not support mode '{mode}'")]
    UnsupportedMode { mode: HandoffMode },
    #[error("target resolution is ambiguous; choose one of {} candidates", choices.len())]
    AmbiguousTarget { choices: Vec<DestinationTarget> },
    #[error("target '{target_id}' is unavailable: {reason}")]
    TargetUnavailable { target_id: String, reason: String },
    #[error("not authorized: {reason}")]
    Unauthorized { reason: String },
    #[error("destination is unreachable: {reason}")]
    Offline { reason: String },
    #[error("bundle exceeds destination limit ({size_bytes} > {limit_bytes} bytes)")]
    Oversized { size_bytes: u64, limit_bytes: u64 },
    #[error("destination cannot accept this content: {reason}")]
    UnsupportedContent { reason: String },
    #[error("{0}")]
    Other(String),
}

/// The seam a destination integration implements.
///
/// Implementations own their own transport (HTTP API, SDK/CLI, MCP, ...),
/// authentication, and target semantics. ctx only supplies neutral values and
/// renders receipts.
pub trait Destination {
    /// Discover targets, optionally filtered by a query string and informed
    /// by neutral Current Context hints. Ambiguity is resolved by the caller
    /// presenting choices, never by guessing.
    fn list_targets(
        &self,
        query: Option<&str>,
        hints: &TargetHints,
    ) -> Result<Vec<DestinationTarget>, HandoffError>;

    /// Perform a deliberate stage/send handoff. Retrying with the same
    /// `request_id` must be idempotent.
    fn handoff(&self, request: &HandoffRequest) -> Result<HandoffReceipt, HandoffError>;
}

fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::current::{CurrentContext, CurrentContextReport};

    fn sample_request(request_id: &str) -> HandoffRequest {
        HandoffRequest {
            request_id: request_id.to_string(),
            bundle: BundlePin {
                bundle_id: "2026-06-24T10-41-03Z_7hf3k2_build-slides".to_string(),
                archive_hash: Some(FileHash {
                    algorithm: "sha256".to_string(),
                    value: "abc123".to_string(),
                }),
                archive_path: Some("/tmp/bundle.ctx".to_string()),
            },
            instruction: Some("Build the deck".to_string()),
            target_id: "target-1".to_string(),
            mode: HandoffMode::Send,
            provenance: HandoffProvenance {
                producer: Some("ctx".to_string()),
                producer_version: Some("0.1.0".to_string()),
                host: None,
            },
            created_at: OffsetDateTime::now_utc(),
        }
    }

    /// In-memory fake destination proving ctx-core needs no product
    /// integration: discovery, stage/send receipts, idempotent retries, and
    /// explicit ambiguity.
    struct FakeDestination {
        targets: Vec<DestinationTarget>,
        received: RefCell<HashMap<String, HandoffReceipt>>,
        supports_stage: bool,
    }

    impl FakeDestination {
        fn new() -> Self {
            Self {
                targets: vec![
                    DestinationTarget {
                        id: "target-1".to_string(),
                        title: "Alpha".to_string(),
                        description: None,
                    },
                    DestinationTarget {
                        id: "target-2".to_string(),
                        title: "Beta".to_string(),
                        description: Some("second target".to_string()),
                    },
                ],
                received: RefCell::new(HashMap::new()),
                supports_stage: true,
            }
        }
    }

    impl Destination for FakeDestination {
        fn list_targets(
            &self,
            query: Option<&str>,
            _hints: &TargetHints,
        ) -> Result<Vec<DestinationTarget>, HandoffError> {
            Ok(self
                .targets
                .iter()
                .filter(|t| {
                    query.is_none_or(|q| {
                        t.title.to_lowercase().contains(&q.to_lowercase()) || t.id == q
                    })
                })
                .cloned()
                .collect())
        }

        fn handoff(&self, request: &HandoffRequest) -> Result<HandoffReceipt, HandoffError> {
            if request.mode == HandoffMode::Stage && !self.supports_stage {
                return Err(HandoffError::UnsupportedMode { mode: request.mode });
            }
            if let Some(existing) = self.received.borrow().get(&request.request_id) {
                return Ok(existing.clone());
            }
            let target = self
                .targets
                .iter()
                .find(|t| t.id == request.target_id)
                .cloned()
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
                    reference: format!("fake://{}", request.request_id),
                }],
                warnings: Vec::new(),
                rejection_reason: None,
                completed_at: OffsetDateTime::now_utc(),
            };
            self.received
                .borrow_mut()
                .insert(request.request_id.clone(), receipt.clone());
            Ok(receipt)
        }
    }

    #[test]
    fn request_and_receipt_roundtrip_json() {
        let request = sample_request("req-1");
        let json = serde_json::to_string(&request).expect("serialize request");
        let back: HandoffRequest = serde_json::from_str(&json).expect("deserialize request");
        assert_eq!(back, request);

        let receipt = HandoffReceipt {
            request_id: "req-1".to_string(),
            outcome: HandoffOutcome::Staged,
            target: DestinationTarget {
                id: "t".to_string(),
                title: "T".to_string(),
                description: None,
            },
            remote_refs: vec![RemoteRef {
                label: None,
                reference: "opaque-ref".to_string(),
            }],
            warnings: vec!["slow".to_string()],
            rejection_reason: None,
            completed_at: OffsetDateTime::now_utc(),
        };
        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let back: HandoffReceipt = serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(back, receipt);
    }

    #[test]
    fn conformance_fixtures_roundtrip() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let fixture_dir = std::path::Path::new(&manifest_dir)
            .join("..")
            .join("examples")
            .join("handoff");
        let request: HandoffRequest = serde_json::from_str(
            &std::fs::read_to_string(fixture_dir.join("request.json")).expect("read request"),
        )
        .expect("parse request fixture");
        assert_eq!(request.mode, HandoffMode::Send);
        assert!(request.bundle.archive_hash.is_some());

        let receipt: HandoffReceipt = serde_json::from_str(
            &std::fs::read_to_string(fixture_dir.join("receipt.json")).expect("read receipt"),
        )
        .expect("parse receipt fixture");
        assert_eq!(receipt.request_id, request.request_id);
        assert_eq!(receipt.outcome, HandoffOutcome::Sent);

        let targets: Vec<DestinationTarget> = serde_json::from_str(
            &std::fs::read_to_string(fixture_dir.join("targets.json")).expect("read targets"),
        )
        .expect("parse targets fixture");
        assert!(targets.iter().any(|t| t.id == request.target_id));
    }

    #[test]
    fn fake_destination_discovers_and_hands_off() {
        let dest = FakeDestination::new();
        let targets = dest.list_targets(None, &TargetHints::default()).unwrap();
        assert_eq!(targets.len(), 2);

        let staged = dest
            .handoff(&HandoffRequest {
                mode: HandoffMode::Stage,
                ..sample_request("req-stage")
            })
            .unwrap();
        assert_eq!(staged.outcome, HandoffOutcome::Staged);

        let sent = dest.handoff(&sample_request("req-send")).unwrap();
        assert_eq!(sent.outcome, HandoffOutcome::Sent);
        assert_eq!(sent.remote_refs.len(), 1);
    }

    #[test]
    fn retry_with_same_request_id_is_idempotent() {
        let dest = FakeDestination::new();
        let first = dest.handoff(&sample_request("req-retry")).unwrap();
        let second = dest.handoff(&sample_request("req-retry")).unwrap();
        assert_eq!(first, second);
        assert_eq!(dest.received.borrow().len(), 1);
    }

    #[test]
    fn unsupported_mode_fails_explicitly() {
        let dest = FakeDestination {
            supports_stage: false,
            ..FakeDestination::new()
        };
        let err = dest
            .handoff(&HandoffRequest {
                mode: HandoffMode::Stage,
                ..sample_request("req-mode")
            })
            .unwrap_err();
        assert!(matches!(
            err,
            HandoffError::UnsupportedMode {
                mode: HandoffMode::Stage
            }
        ));
    }

    #[test]
    fn unknown_target_never_falls_back() {
        let dest = FakeDestination::new();
        let err = dest
            .handoff(&HandoffRequest {
                target_id: "missing".to_string(),
                ..sample_request("req-missing")
            })
            .unwrap_err();
        assert!(matches!(err, HandoffError::TargetUnavailable { .. }));
        assert!(dest.received.borrow().is_empty());
    }

    #[test]
    fn hints_derive_from_current_context() {
        let context = CurrentContext::default().apply_report(CurrentContextReport {
            cwd: Some("/home/me/proj".to_string()),
            project: Some("proj".to_string()),
            app: Some("Ghostty".to_string()),
            url: Some("https://example.com".to_string()),
            ..Default::default()
        });
        let hints = TargetHints::from_current_context(&context);
        assert_eq!(hints.cwd.as_deref(), Some("/home/me/proj"));
        assert_eq!(hints.project.as_deref(), Some("proj"));
        assert_eq!(hints.app.as_deref(), Some("Ghostty"));
        assert_eq!(hints.url.as_deref(), Some("https://example.com"));
    }
}
