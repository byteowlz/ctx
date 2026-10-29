//! Fixed action catalog for the headless command surface.
//!
//! One catalog, shared by the CLI, voice and the bar: every entry is a fixed
//! argv template with stable id, labels and bilingual aliases. Nothing here
//! interpolates user text into a command, and nothing executes on its own.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Macos,
}

#[derive(Debug, Clone, Copy)]
pub struct Action {
    pub id: &'static str,
    /// Short English label shown in previews.
    pub label: &'static str,
    /// Short German label.
    pub label_de: &'static str,
    /// Words that mean this action, in English and German.
    pub aliases: &'static [&'static str],
    /// Fixed argv template; never contains user text.
    pub argv: &'static [&'static str],
}

/// macOS v1. Site actions open a fixed URL; settings actions open a pane;
/// toggles use `osascript` with a fixed script.
pub const ACTIONS: &[Action] = &[
    Action {
        id: "open.x",
        label: "Open X (Twitter)",
        label_de: "X (Twitter) öffnen",
        aliases: &[
            "twitter",
            "x",
            "x.com",
            "geh zu twitter",
            "zu twitter",
            "open twitter",
        ],
        argv: &["open", "https://x.com"],
    },
    Action {
        id: "open.github",
        label: "Open GitHub",
        label_de: "GitHub öffnen",
        aliases: &["github", "geh zu github", "open github"],
        argv: &["open", "https://github.com"],
    },
    Action {
        id: "open.mail",
        label: "Open Mail",
        label_de: "Mail öffnen",
        aliases: &["mail", "email", "e-mail", "open mail"],
        argv: &["open", "-a", "Mail"],
    },
    Action {
        id: "appearance.dark",
        label: "Switch to dark mode",
        label_de: "Dunkelmodus einschalten",
        aliases: &["dark mode", "dark", "dunkelmodus", "dunkel"],
        argv: &[
            "osascript",
            "-e",
            "tell application \"System Events\" to tell appearance preferences to set dark mode to true",
        ],
    },
    Action {
        id: "appearance.light",
        label: "Switch to light mode",
        label_de: "Hellmodus einschalten",
        aliases: &["light mode", "light", "hellmodus", "hell"],
        argv: &[
            "osascript",
            "-e",
            "tell application \"System Events\" to tell appearance preferences to set dark mode to false",
        ],
    },
    Action {
        id: "sound.mute",
        label: "Mute sound output",
        label_de: "Ton stummschalten",
        aliases: &["mute", "stumm", "stummschalten", "ton aus"],
        argv: &[
            "osascript",
            "-e",
            "set volume output muted (not (output muted of (get volume settings)))",
        ],
    },
    Action {
        id: "sound.volume",
        label: "Volume +10",
        label_de: "Lautstärke +10",
        aliases: &["volume", "lautstärke", "lauter", "louder"],
        argv: &[
            "osascript",
            "-e",
            "set v to output volume of (get volume settings)",
            "-e",
            "if v < 100 then set volume output volume (v + 10)",
        ],
    },
    Action {
        id: "appearance.open",
        label: "Open Appearance settings",
        label_de: "Darstellungseinstellungen öffnen",
        aliases: &["appearance settings", "darstellung", "appearance"],
        argv: &[
            "open",
            "x-apple.systempreferences:com.apple.Appearance-Settings.extension",
        ],
    },
    Action {
        id: "display.open",
        label: "Open Display settings",
        label_de: "Bildschirmeinstellungen öffnen",
        aliases: &[
            "display settings",
            "bildschirm",
            "bildschirmeinstellungen",
            "displays",
            "monitor",
        ],
        argv: &[
            "open",
            "x-apple.systempreferences:com.apple.Displays-Settings.extension",
        ],
    },
    Action {
        id: "notifications.open",
        label: "Open Notifications settings",
        label_de: "Mitteilungseinstellungen öffnen",
        aliases: &["notifications", "mitteilungen", "do not disturb"],
        argv: &[
            "open",
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
        ],
    },
    Action {
        id: "accessibility.open",
        label: "Open Accessibility settings",
        label_de: "Bedienungshilfen öffnen",
        aliases: &["accessibility", "bedienungshilfen"],
        argv: &[
            "open",
            "x-apple.systempreferences:com.apple.Accessibility-Settings.extension",
        ],
    },
    Action {
        id: "privacy.open",
        label: "Open Privacy settings",
        label_de: "Datenschutz öffnen",
        aliases: &["privacy", "datenschutz", "security"],
        argv: &[
            "open",
            "x-apple.systempreferences:com.apple.Privacy-Security-Settings.extension",
        ],
    },
];

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Action {
    pub fn by_id(id: &str) -> Option<&'static Action> {
        ACTIONS.iter().find(|action| action.id == id)
    }

    /// Does this alias match the utterance? Multi-word aliases must appear as a
    /// contiguous phrase; single words must appear as whole words.
    fn matches_alias(&self, utterance: &str) -> Option<usize> {
        let utterance = format!(" {utterance} ");
        self.aliases
            .iter()
            .filter_map(|alias| {
                let alias = normalize(alias);
                format!(" {utterance} ")
                    .contains(&format!(" {alias} "))
                    .then_some(alias.len())
            })
            .max()
    }

    /// Preview payload: no user text reaches argv.
    pub fn preview(&self) -> Preview {
        Preview {
            id: self.id,
            label: self.label,
            label_de: self.label_de,
            argv: self.argv,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub id: &'static str,
    pub label: &'static str,
    pub label_de: &'static str,
    pub argv: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub utterance: String,
    /// Matched action, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<Preview>,
    /// How it was resolved: a fixed alias, or the decision model.
    pub method: &'static str,
    /// Model confidence for `method = "model"`; absent for aliases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    /// Why nothing matched, for honest previews.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Remove a leading wake phrase ("hey oqto, ...") so the rest can be routed.
/// Returns the remainder and whether a wake phrase was found.
pub fn strip_wake(utterance: &str, wake: &[&str]) -> (String, bool) {
    let normalized = normalize(utterance);
    for phrase in wake {
        let phrase = normalize(phrase);
        if normalized == phrase {
            return (String::new(), true);
        }
        if normalized.starts_with(&format!("{phrase} ")) {
            let rest = normalized[phrase.len()..].trim().to_string();
            return (rest, true);
        }
    }
    (normalized, false)
}

/// Words that may appear in a spoken command without changing its meaning.
const FILLER: &[&str] = &[
    "please",
    "can",
    "could",
    "would",
    "you",
    "the",
    "a",
    "an",
    "my",
    "to",
    "on",
    "off",
    "up",
    "down",
    "turn",
    "set",
    "switch",
    "open",
    "go",
    "show",
    "make",
    "it",
    "and",
    "bitte",
    "kannst",
    "du",
    "den",
    "die",
    "das",
    "dem",
    "mein",
    "auf",
    "um",
    "zu",
    "zum",
    "zur",
    "mach",
    "oeffne",
    "öffne",
    "einschalten",
    "ausschalten",
    "geh",
    "wechsle",
    "wechsel",
    "an",
    "aus",
    "hoch",
    "runter",
    "mal",
    "schnell",
    "jetzt",
    "mode",
    "settings",
    "einstellungen",
];
/// Clause and reported-speech markers: anything conditional or quoted is
/// prose, not a command, however many keywords it contains.
const CLAUSE: &[&str] = &[
    "when", "if", "that", "because", "while", "before", "after", "said", "says", "told", "tell",
    "should", "wenn", "falls", "dass", "weil", "bevor", "nachdem", "ob", "sagte", "gesagt", "soll",
    "sollte", "sage",
];

/// A spoken command is short and made only of catalog words, filler and
/// optional courtesy. Long sentences, conditionals and reported speech are
/// never commands, no matter which keyword they contain.
pub fn command_shaped(normalized: &str) -> bool {
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 10 {
        return false;
    }
    if tokens.iter().any(|token| CLAUSE.contains(token)) {
        return false;
    }
    // Aliases are compared in the same normalized form as the utterance, so
    // "x.com" contributes the words "x" and "com".
    let normalized_aliases: Vec<String> = ACTIONS
        .iter()
        .flat_map(|action| action.aliases.iter())
        .map(|alias| normalize(alias))
        .collect();
    let alias_words: std::collections::HashSet<&str> = normalized_aliases
        .iter()
        .flat_map(|alias| alias.split_whitespace())
        .collect();
    tokens.iter().all(|token| {
        FILLER.contains(token)
            || alias_words.contains(token)
            || token.chars().all(|c| c.is_ascii_digit())
    })
}

/// Deterministic resolution against the fixed catalog. The longest matching
/// alias wins, so "dark mode" beats "mode" and "display settings" beats
/// "display".
pub fn resolve(utterance: &str) -> Resolution {
    let normalized = normalize(utterance);
    if normalized.is_empty() {
        return Resolution {
            utterance: utterance.to_string(),
            action: None,
            method: "alias",
            probability: None,
            note: Some("empty utterance".into()),
        };
    }
    let shaped = command_shaped(&normalized);
    let best = ACTIONS
        .iter()
        .filter(|_| shaped)
        .filter_map(|action| {
            action
                .matches_alias(&normalized)
                .map(|score| (score, action))
        })
        .max_by_key(|(score, action)| (*score, std::cmp::Reverse(action.id)));
    match best {
        Some((_, action)) => Resolution {
            utterance: utterance.to_string(),
            action: Some(action.preview()),
            method: "alias",
            probability: None,
            note: None,
        },
        None => Resolution {
            utterance: utterance.to_string(),
            action: None,
            method: "alias",
            probability: None,
            note: Some(if shaped {
                "no fixed action matches; a decision model may route it".into()
            } else {
                "not a command-shaped utterance (prose, conditional or reported speech)".into()
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_and_german_phrases_resolve_to_one_action() {
        for (utterance, id) in [
            ("geh zu Twitter", "open.x"),
            ("open twitter", "open.x"),
            ("zu x.com", "open.x"),
            ("Geh zu GitHub", "open.github"),
            ("switch to dark mode", "appearance.dark"),
            ("mach den Dunkelmodus an", "appearance.dark"),
            ("Lautstärke hoch", "sound.volume"),
            ("stummschalten", "sound.mute"),
        ] {
            assert_eq!(
                resolve(utterance).action.map(|a| a.id),
                Some(id),
                "{utterance}"
            );
        }
    }

    #[test]
    fn display_settings_beat_a_bare_display_alias() {
        assert_eq!(
            resolve("open display settings").action.unwrap().id,
            "display.open"
        );
        assert_eq!(
            resolve("Bildschirmeinstellungen").action.unwrap().id,
            "display.open"
        );
    }

    #[test]
    fn conditionals_and_reported_speech_are_never_commands() {
        for prose in [
            "Mute your microphone when you are not speaking.",
            "She said turn on dark mode, it is easier on the eyes.",
            "Tell Paul that the volume button is broken again.",
            "Turn left at the second traffic light and park behind the building.",
            "Schalte dein Mikrofon stumm, wenn du nicht sprichst.",
            "Er hat gesagt, ich soll den Bildschirm sperren, wenn ich weggehe.",
            "Denk daran, die Tür abzuschließen, wenn du gehst.",
        ] {
            assert!(resolve(prose).action.is_none(), "{prose}");
        }
    }

    #[test]
    fn ordinary_prose_does_not_become_a_command() {
        for prose in [
            "please send me the report by Friday",
            "the certificate must be renewed before january",
            "thanks, that looks good",
            "I will retry with the password prompt",
        ] {
            let resolved = resolve(prose);
            assert!(resolved.action.is_none(), "{prose}");
            assert!(resolved.note.is_some());
        }
    }

    #[test]
    fn wake_phrases_are_stripped_before_routing() {
        let (rest, found) = strip_wake("Hey oqto, geh zu Twitter", &["hey oqto"]);
        assert!(found);
        assert_eq!(rest, "geh zu twitter");
        assert_eq!(resolve(&rest).action.unwrap().id, "open.x");
        let (rest, found) = strip_wake("hey oqto", &["hey oqto"]);
        assert!(found && rest.is_empty());
        let (rest, found) = strip_wake("geh zu twitter", &["hey oqto"]);
        assert!(!found && rest == "geh zu twitter");
        // A near phrase must not be swallowed.
        let (_, found) = strip_wake("hey oqtos cousin", &["hey oqto"]);
        assert!(!found);
    }

    #[test]
    fn templates_are_static_and_never_interpolate_user_text() {
        for action in ACTIONS {
            assert!(!action.argv.is_empty());
            for part in action.argv {
                assert!(!part.contains("{}"), "{}", action.id);
                assert!(!part.contains('$'), "{}", action.id);
            }
            assert!(!action.label.is_empty() && !action.label_de.is_empty());
            assert_eq!(Action::by_id(action.id).map(|a| a.id), Some(action.id));
        }
    }
}
