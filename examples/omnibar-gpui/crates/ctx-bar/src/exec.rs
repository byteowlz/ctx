//! Human-confirmed execution of a fixed macOS action whitelist.
//!
//! Safety contract: the model, probabilities, timers and dwell can select and
//! preview, but only an explicit user gesture (Enter on a preview or the Run
//! control) runs something, and every command is a fixed argv template keyed
//! by catalog id. Query text, item text and model output are never inserted
//! into a command. Other platforms have no backend yet and stay preview-only.

use std::process::Command;
use std::time::Instant;

pub struct Action {
    pub summary: &'static str,
    pub argv: Vec<&'static str>,
}

pub struct Outcome {
    pub ok: bool,
    pub duration_ms: u128,
    pub detail: String,
}

/// Fixed macOS whitelist. Values are static templates only.
#[cfg(target_os = "macos")]
pub fn action_for(item_id: &str) -> Option<Action> {
    let (summary, argv): (&'static str, Vec<&'static str>) = match item_id {
        "appearance.dark" => (
            "dark mode on",
            vec![
                "osascript",
                "-e",
                "tell application \"System Events\" to tell appearance preferences to set dark mode to true",
            ],
        ),
        "appearance.light" => (
            "dark mode off",
            vec![
                "osascript",
                "-e",
                "tell application \"System Events\" to tell appearance preferences to set dark mode to false",
            ],
        ),
        "sound.mute" => (
            "output mute toggled",
            vec![
                "osascript",
                "-e",
                "set volume output muted (not (output muted of (get volume settings)))",
            ],
        ),
        "sound.volume" => (
            "volume +10",
            vec![
                "osascript",
                "-e",
                "set v to output volume of (get volume settings)",
                "-e",
                "if v < 100 then set volume output volume (v + 10)",
            ],
        ),
        "appearance.open" => (
            "opened Appearance settings",
            vec![
                "open",
                "x-apple.systempreferences:com.apple.Appearance-Settings.extension",
            ],
        ),
        "display.open" => (
            "opened Display settings",
            vec![
                "open",
                "x-apple.systempreferences:com.apple.Displays-Settings.extension",
            ],
        ),
        "notifications.open" => (
            "opened Notifications settings",
            vec![
                "open",
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            ],
        ),
        "accessibility.open" => (
            "opened Accessibility settings",
            vec![
                "open",
                "x-apple.systempreferences:com.apple.Accessibility-Settings.extension",
            ],
        ),
        "privacy.open" => (
            "opened Privacy settings",
            vec![
                "open",
                "x-apple.systempreferences:com.apple.Privacy-Security-Settings.extension",
            ],
        ),
        _ => return None,
    };
    Some(Action { summary, argv })
}

#[cfg(not(target_os = "macos"))]
pub fn action_for(_item_id: &str) -> Option<Action> {
    None
}

/// Runs one fixed template. Never uses a shell; output is truncated.
pub fn run(action: &Action) -> Outcome {
    let started = Instant::now();
    let (ok, detail) = match Command::new(action.argv[0])
        .args(&action.argv[1..])
        .output()
    {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if text.is_empty() {
                text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            if text.len() > 200 {
                text.truncate(200);
            }
            (
                output.status.success(),
                if text.is_empty() {
                    format!("exit {}", output.status)
                } else {
                    text
                },
            )
        }
        Err(error) => (false, error.to_string()),
    };
    Outcome {
        ok,
        duration_ms: started.elapsed().as_millis(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_is_fixed_and_total() {
        for id in [
            "appearance.dark",
            "appearance.light",
            "sound.mute",
            "sound.volume",
            "appearance.open",
            "display.open",
            "notifications.open",
            "accessibility.open",
            "privacy.open",
        ] {
            let action = action_for(id).expect("whitelisted id");
            assert!(!action.summary.is_empty());
            assert!(!action.argv.is_empty());
            for argument in &action.argv {
                assert!(!argument.contains("{"), "no interpolation in {argument}");
            }
        }
        for id in [
            "sound.apps",
            "fixture.appearance",
            "",
            "appearance.dark; rm",
        ] {
            assert!(action_for(id).is_none(), "{id} must not execute");
        }
    }

    #[test]
    fn outcome_reports_real_result() {
        #[cfg(target_os = "macos")]
        {
            let outcome = run(&action_for("sound.volume").unwrap());
            assert!(outcome.ok, "{}", outcome.detail);
        }
        let missing = run(&Action {
            summary: "missing",
            argv: vec!["ctx-nonexistent-binary-xyz"],
        });
        assert!(!missing.ok);
    }
}
