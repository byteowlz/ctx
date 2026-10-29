//! Read-only focus signals for the dictation-vs-command gate.
//!
//! On macOS the focused UI element's *role* plus whether its value is
//! settable separates a real text target from a terminal without reading any
//! text: a terminal is an `AXTextArea` whose value is not settable. Combined
//! with herdr's pane/agent inventory (safe, JSON) this answers "would speech
//! here be dictation or a command". No text, window titles, clipboard or
//! screenshots are read, and nothing here changes focus.
use std::process::Command;

use serde::Serialize;

/// What kind of input target currently owns the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// A real editable text field or composer.
    Text,
    /// A terminal surface (its AX value is not settable).
    Terminal,
    /// Nothing focused, or no accessibility answer at all.
    None,
    /// An app or element that does not answer accessibility (Electron,
    /// Chromium pages, locked screen).
    Unknown,
}

/// What speech at this focus should do. Policy only; nothing executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Dictation,
    Command,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Element {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subrole: Option<String>,
    pub value_settable: bool,
    /// Ghostty and other terminals identify their surface here; treated as a
    /// terminal marker and never shown.
    pub terminal_marker: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HerdrPane {
    pub pane_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub agent_status: Option<String>,
    pub foreground_cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub target: Target,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<Element>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub herdr: Option<HerdrPane>,
    pub decision: Decision,
    pub reason: &'static str,
}

/// The gate. Pure, so it can be tested and reviewed without a desktop.
///
/// Rules, in order: a real text target is dictation; a terminal running an
/// agent is dictation to that agent; a terminal without an agent is dictation
/// with a "run as command?" affordance (never auto-submitted); nothing or an
/// unresolvable target defaults to dictation, because typed text is visible
/// and undoable while a wrong command is not.
pub fn decide(target: Target, terminal_agent: Option<bool>) -> (Decision, &'static str) {
    match (target, terminal_agent) {
        (Target::Text, _) => (Decision::Dictation, "caret in an editable text target"),
        (Target::Terminal, Some(true)) => (Decision::Dictation, "terminal pane runs an agent"),
        (Target::Terminal, Some(false)) => (
            Decision::Dictation,
            "terminal shell: dictation only, offer a command chip",
        ),
        (Target::Terminal, None) => (Decision::Dictation, "terminal without a known pane"),
        (Target::None, _) => (Decision::Command, "no editable focus"),
        (Target::Unknown, _) => (Decision::Unknown, "accessibility gave no answer"),
    }
}

/// Classify an element from its role, settable flag and help text.
pub fn classify(role: &str, value_settable: bool, help: Option<&str>) -> (Target, Option<Element>) {
    let terminal_help = help.is_some_and(|h| h.to_ascii_lowercase().contains("terminal"));
    let element = Element {
        role: role.to_string(),
        subrole: None,
        value_settable,
        terminal_marker: terminal_help,
    };
    let text_role = matches!(
        role,
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField"
    );
    let target = if value_settable && text_role {
        Target::Text
    } else if (!value_settable && role == "AXTextArea") || terminal_help {
        Target::Terminal
    } else if text_role {
        // A text-ish role without a settable value (read-only view, preview).
        Target::Terminal
    } else {
        Target::None
    };
    (target, Some(element))
}

/// Parse `herdr api snapshot` JSON into the focused pane, if herdr is running.
pub fn herdr_from_snapshot(json: &str) -> Option<HerdrPane> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let snapshot = &value["result"]["snapshot"];
    let focused = snapshot["focused_pane_id"].as_str()?;
    let pane = snapshot["panes"]
        .as_array()?
        .iter()
        .find(|pane| pane["pane_id"].as_str() == Some(focused))?;
    let agent = pane["agent"].as_str().filter(|a| !a.is_empty());
    Some(HerdrPane {
        pane_id: focused.to_string(),
        agent: agent.map(str::to_string),
        agent_status: pane["agent_status"].as_str().map(str::to_string),
        foreground_cwd: pane["foreground_cwd"].as_str().map(str::to_string),
    })
}

/// Ask herdr for its snapshot; absent herdr is not an error.
pub fn herdr_pane() -> Option<HerdrPane> {
    // A non-interactive shell (ssh, an app) may not have Homebrew on PATH.
    let mut candidates = vec![std::path::PathBuf::from("herdr")];
    candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/herdr"));
    candidates.push(std::path::PathBuf::from("/usr/local/bin/herdr"));
    for candidate in candidates {
        let Ok(output) = Command::new(&candidate).args(["api", "snapshot"]).output() else {
            continue;
        };
        if output.status.success()
            && let Some(pane) = herdr_from_snapshot(&String::from_utf8_lossy(&output.stdout))
        {
            return Some(pane);
        }
    }
    None
}

/// Collect the current report. Read-only; never raises for a missing answer.
pub fn report() -> FocusReport {
    let (app, pid) = frontmost();
    let element = pid.and_then(focused_element);
    let (target, element) = match &element {
        Some((role, settable, help)) => classify(role, *settable, help.as_deref()),
        None => (
            if pid.is_some() {
                Target::Unknown
            } else {
                Target::None
            },
            None,
        ),
    };
    let herdr = if target == Target::Terminal {
        herdr_pane()
    } else {
        None
    };
    let terminal_agent = herdr.as_ref().map(|pane| pane.agent.is_some());
    let (decision, reason) = decide(target, terminal_agent);
    FocusReport {
        app,
        pid,
        target,
        element,
        herdr,
        decision,
        reason,
    }
}

/// Frontmost application name and pid, via the same window source ctx already
/// uses. Title and other content are not read.
fn frontmost() -> (Option<String>, Option<u32>) {
    #[cfg(target_os = "macos")]
    {
        match x_win::get_active_window() {
            Ok(window) if window.info.process_id != 0 => {
                let name = window.info.name.clone();
                (
                    (!name.is_empty()).then_some(name),
                    Some(window.info.process_id),
                )
            }
            Ok(_) => (None, None),
            Err(_) => (None, None),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        (None, None)
    }
}

/// Focused element of `pid`: role, whether its value is settable, and its
/// accessibility help string. macOS only; other platforms report nothing.
#[cfg(target_os = "macos")]
fn focused_element(pid: u32) -> Option<(String, bool, Option<String>)> {
    use accessibility_sys::{
        AXUIElementCopyAttributeValue, AXUIElementCreateApplication,
        AXUIElementIsAttributeSettable, kAXFocusedUIElementAttribute, kAXHelpAttribute,
        kAXRoleAttribute, kAXValueAttribute,
    };
    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    /// Reads one string attribute (role, help). Never `AXValue`, titles or
    /// selected text.
    fn string_attr(element: accessibility_sys::AXUIElementRef, name: &str) -> Option<String> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: read-only AX query; the result is a create-rule reference.
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != 0 || value.is_null() {
            return None;
        }
        // SAFETY: AX returns a +1 retained CFString for string attributes.
        Some(unsafe { CFString::wrap_under_create_rule(value as CFStringRef) }.to_string())
    }

    // SAFETY: read-only AX queries on a live process's accessibility element.
    // No values, titles or text are read; only role/help strings and a
    // settable flag. The element is released with the process.
    unsafe {
        let app = AXUIElementCreateApplication(pid as i32);
        if app.is_null() {
            return None;
        }
        let attribute = CFString::new(kAXFocusedUIElementAttribute);
        let mut focused: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(app, attribute.as_concrete_TypeRef(), &mut focused) != 0
            || focused.is_null()
        {
            return None;
        }
        let focused_ref = focused as accessibility_sys::AXUIElementRef;
        let role = string_attr(focused_ref, kAXRoleAttribute)?;
        let help = string_attr(focused_ref, kAXHelpAttribute);
        let value = CFString::new(kAXValueAttribute);
        let mut settable: std::ffi::c_uchar = 0;
        let settable = AXUIElementIsAttributeSettable(
            focused as accessibility_sys::AXUIElementRef,
            value.as_concrete_TypeRef(),
            &mut settable,
        ) == 0
            && settable != 0;
        Some((role, settable, help))
    }
}

#[cfg(not(target_os = "macos"))]
fn focused_element(_pid: u32) -> Option<(String, bool, Option<String>)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settable_text_roles_are_text_targets_and_terminals_are_not() {
        assert_eq!(classify("AXTextField", true, None).0, Target::Text);
        assert_eq!(classify("AXComboBox", true, None).0, Target::Text);
        assert_eq!(
            classify("AXTextArea", false, Some("Terminal content area")).0,
            Target::Terminal
        );
        // A terminal without the help marker still has a non-settable value.
        assert_eq!(classify("AXTextArea", false, None).0, Target::Terminal);
        assert_eq!(classify("AXGroup", false, None).0, Target::None);
        assert_eq!(classify("AXWindow", false, None).0, Target::None);
    }

    #[test]
    fn gate_defaults_to_dictation_and_only_commands_without_text_focus() {
        assert_eq!(decide(Target::Text, None).0, Decision::Dictation);
        assert_eq!(decide(Target::Text, Some(false)).0, Decision::Dictation);
        assert_eq!(decide(Target::Terminal, Some(true)).0, Decision::Dictation);
        assert_eq!(decide(Target::Terminal, Some(false)).0, Decision::Dictation);
        assert_eq!(decide(Target::Terminal, None).0, Decision::Dictation);
        assert_eq!(decide(Target::Unknown, None).0, Decision::Unknown);
        assert_eq!(decide(Target::None, None).0, Decision::Command);
    }

    #[test]
    fn herdr_snapshot_yields_the_focused_pane_and_its_agent() {
        let snapshot = r#"{"result":{"snapshot":{
            "focused_pane_id":"w1:p2",
            "panes":[
              {"pane_id":"w1:p1","agent":"pi","agent_status":"idle","foreground_cwd":"/a"},
              {"pane_id":"w1:p2","agent":"","agent_status":null,"foreground_cwd":"/b"}
            ]}}}"#;
        let pane = herdr_from_snapshot(snapshot).expect("focused pane");
        assert_eq!(pane.pane_id, "w1:p2");
        assert_eq!(pane.agent, None, "empty agent means a plain shell");
        assert_eq!(pane.foreground_cwd.as_deref(), Some("/b"));

        let with_agent = snapshot.replace(r#""agent":"""#, r#""agent":"pi""#);
        assert_eq!(
            herdr_from_snapshot(&with_agent).unwrap().agent.as_deref(),
            Some("pi")
        );
        assert!(herdr_from_snapshot("{}").is_none());
        assert!(herdr_from_snapshot("not json").is_none());
    }
}
