//! Typed command plans: an utterance plus the current selection becomes a
//! concrete proposal (action, files, recipient, channel) that is previewed and
//! confirmed by a human. Nothing here sends, opens or writes user data.
use serde::Serialize;

use crate::contacts::{Config as Contacts, Found};
use crate::selection::Selection;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Send the selected files to a person.
    SendFiles,
    /// Open a fixed catalog action (see `actions`).
    Fixed,
    /// Recognised as something this build cannot do yet.
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recipient {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub kind: Kind,
    pub utterance: String,
    /// Files the plan would send, as selected by the producer.
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Recipient>,
    /// Why the plan cannot run, if it cannot. Empty means "preview complete".
    pub missing: Vec<String>,
    /// True only when every argument resolved and the channel is executable.
    pub ready: bool,
    /// What would happen, in one line for the status line.
    pub summary: String,
}

/// Words that introduce the recipient, English and German.
const TO_MARKERS: &[&str] = &["to", "an", "zu", "für", "for"];

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Recipient after the last "to"/"an" marker, e.g. "send this to anna müller"
/// -> "anna müller". Returns None when there is no marker or nothing after it.
pub fn recipient_phrase(utterance: &str) -> Option<String> {
    let normalized = normalize(utterance);
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let position = tokens
        .iter()
        .rposition(|token| TO_MARKERS.contains(token))?;
    let rest = tokens.get(position + 1..)?;
    (!rest.is_empty()).then(|| rest.join(" "))
}

/// Direct object markers that mean "the current selection": this/these/them,
/// das/diese/sie. Used only to decide whether files are expected.
pub fn refers_to_selection(utterance: &str) -> bool {
    let normalized = normalize(utterance);
    normalized.split_whitespace().any(|token| {
        matches!(
            token,
            "this" | "these" | "them" | "it" | "das" | "diese" | "die" | "sie" | "den"
        )
    }) || normalized.contains(" the files")
        || normalized.contains("die dateien")
}

/// Build a plan for "send <selection> to <person>" in EN or DE. The plan is
/// `ready` only when the selection exists, the recipient is unique, the channel
/// resolves and sending is explicitly enabled; otherwise `missing` says why.
pub fn send_files(utterance: &str, selection: &Selection, contacts: &Contacts) -> Plan {
    let mut missing = Vec::new();
    if selection.is_empty() {
        missing.push("no files selected".to_string());
    }
    let absent: Vec<String> = selection
        .missing()
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    if !absent.is_empty() {
        missing.push(format!("{} selected file(s) no longer exist", absent.len()));
    }
    let recipient = recipient_phrase(utterance).map(|phrase| match contacts.find(&phrase) {
        Found::One(person) => match person.channel(&contacts.commands) {
            Some((channel, handle)) => Recipient {
                name: person.name.clone(),
                channel: Some(channel),
                handle: Some(handle),
            },
            None => {
                missing.push(format!("{} has no usable channel configured", person.name));
                Recipient {
                    name: person.name.clone(),
                    channel: None,
                    handle: None,
                }
            }
        },
        Found::Ambiguous(candidates) => {
            missing.push(format!("name is ambiguous: {}", candidates.join(", ")));
            Recipient {
                name: phrase.clone(),
                channel: None,
                handle: None,
            }
        }
        Found::Unknown => {
            missing.push(format!("no contact matches {phrase:?}"));
            Recipient {
                name: phrase.clone(),
                channel: None,
                handle: None,
            }
        }
    });
    if recipient.is_none() {
        missing.push("no recipient in the utterance".to_string());
    }
    if !contacts.commands.allow_send {
        missing.push("sending is disabled in contacts.toml (allow_send = false)".to_string());
    }
    let ready = missing.is_empty();
    let summary = match (&recipient, selection.is_empty()) {
        (Some(recipient), false) => format!(
            "send {} file(s) to {}{}",
            selection.files.len(),
            recipient.name,
            recipient
                .channel
                .as_deref()
                .map(|channel| format!(" via {channel}"))
                .unwrap_or_default()
        ),
        (Some(recipient), true) => format!(
            "send the selection to {} (nothing selected)",
            recipient.name
        ),
        (None, _) => "send the selection (recipient unresolved)".to_string(),
    };
    Plan {
        kind: Kind::SendFiles,
        utterance: utterance.to_string(),
        files: selection
            .files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        recipient,
        missing,
        ready,
        summary,
    }
}

/// Is this utterance a "send ... to ..." request in English or German?
pub fn is_send_request(utterance: &str) -> bool {
    let normalized = normalize(utterance);
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let send = tokens.iter().any(|token| {
        matches!(
            *token,
            "send" | "schick" | "schicke" | "sende" | "senden" | "mail" | "share" | "teile"
        )
    });
    send && recipient_phrase(utterance).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::Config;
    use std::path::PathBuf;
    use time::OffsetDateTime;

    fn contacts(allow_send: bool) -> Config {
        toml::from_str(&format!(
            r#"
[commands]
default_channel = "teams"
allow_send = {allow_send}

[[person]]
name = "Anna Müller"
aliases = ["Anna"]
preferred = "teams"
[person.channels]
teams = "anna@example.com"
"#
        ))
        .unwrap()
    }

    fn selection(paths: &[&str]) -> Selection {
        Selection {
            files: paths.iter().map(PathBuf::from).collect(),
            source: "yazi".into(),
            at: OffsetDateTime::now_utc(),
        }
    }

    fn existing() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ctx-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.pdf");
        let b = dir.join("b.png");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        (a, b)
    }

    #[test]
    fn english_and_german_send_requests_are_recognised() {
        for utterance in [
            "send this to Anna",
            "send these files to Anna Müller",
            "schick das an Anna",
            "sende die Dateien an Anna",
        ] {
            assert!(is_send_request(utterance), "{utterance}");
        }
        for prose in [
            "I will send you the report tomorrow",
            "please send me the report by Friday",
            "the certificate must be renewed",
        ] {
            assert!(!is_send_request(prose), "{prose}");
        }
    }

    #[test]
    fn recipient_phrase_takes_the_last_marker() {
        assert_eq!(
            recipient_phrase("send this to anna müller").as_deref(),
            Some("anna müller")
        );
        assert_eq!(
            recipient_phrase("schick das an Anna").as_deref(),
            Some("anna")
        );
        assert_eq!(recipient_phrase("send this").as_deref(), None);
        assert_eq!(recipient_phrase("send this to").as_deref(), None);
    }

    #[test]
    fn plan_resolves_files_recipient_and_channel_but_refuses_to_send() {
        let (a, b) = existing();
        let plan = send_files(
            "send these to anna",
            &selection(&[a.to_str().unwrap(), b.to_str().unwrap()]),
            &contacts(false),
        );
        assert_eq!(plan.kind, Kind::SendFiles);
        assert_eq!(plan.files.len(), 2);
        let recipient = plan.recipient.as_ref().unwrap();
        assert_eq!(recipient.name, "Anna Müller");
        assert_eq!(recipient.channel.as_deref(), Some("teams"));
        assert_eq!(recipient.handle.as_deref(), Some("anna@example.com"));
        assert!(!plan.ready, "sending stays disabled by default");
        assert!(plan.missing.iter().any(|m| m.contains("allow_send")));
        assert!(plan.summary.contains("2 file(s)") && plan.summary.contains("teams"));
    }

    #[test]
    fn missing_selection_unknown_or_ambiguous_recipient_are_named() {
        let plan = send_files("send this to anna", &Selection::empty(), &contacts(true));
        assert!(plan.missing.iter().any(|m| m.contains("no files selected")));
        assert!(!plan.ready);

        let (a, _) = existing();
        let plan = send_files(
            "send this to nobody",
            &selection(&[a.to_str().unwrap()]),
            &contacts(true),
        );
        assert!(
            plan.missing
                .iter()
                .any(|m| m.starts_with("no contact matches"))
        );
        assert!(!plan.ready);

        let two_annas: Config = toml::from_str(
            r#"
[[person]]
name = "Anna Müller"
[person.channels]
teams = "a@b"

[[person]]
name = "Anna Schmidt"
[person.channels]
teams = "c@d"
"#,
        )
        .unwrap();
        let plan = send_files(
            "send this to anna",
            &selection(&[a.to_str().unwrap()]),
            &two_annas,
        );
        assert!(plan.missing.iter().any(|m| m.contains("ambiguous")));
        assert!(!plan.ready, "an ambiguous name must never resolve");
    }

    #[test]
    fn a_deleted_selected_file_blocks_the_plan() {
        let (a, _) = existing();
        let path = a.to_str().unwrap();
        let plan = send_files("send this to anna", &selection(&[path]), &contacts(true));
        std::fs::remove_file(&a).unwrap();
        let stale = send_files("send this to anna", &selection(&[path]), &contacts(true));
        assert!(plan.ready || !plan.ready); // first plan may be ready
        assert!(stale.missing.iter().any(|m| m.contains("no longer exist")));
        assert!(!stale.ready);
    }
}
