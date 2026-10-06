//! GPUI-free identities and input commands for the ten-design native trial.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BarDesign {
    Underline,
    Monolith,
    Lens,
    Signal,
    Prompt,
    #[default]
    DotMatrix,
    Corners,
    Slot,
    Unframed,
    Notch,
}

impl BarDesign {
    pub const ALL: [Self; 10] = [
        Self::Underline,
        Self::Monolith,
        Self::Lens,
        Self::Signal,
        Self::Prompt,
        Self::DotMatrix,
        Self::Corners,
        Self::Slot,
        Self::Unframed,
        Self::Notch,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Underline => "underline",
            Self::Monolith => "monolith",
            Self::Lens => "lens",
            Self::Signal => "signal",
            Self::Prompt => "prompt",
            Self::DotMatrix => "dot-matrix",
            Self::Corners => "corners",
            Self::Slot => "slot",
            Self::Unframed => "unframed",
            Self::Notch => "notch",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Underline => "Underline",
            Self::Monolith => "Monolith",
            Self::Lens => "Lens",
            Self::Signal => "Signal",
            Self::Prompt => "Prompt",
            Self::DotMatrix => "Dot Matrix",
            Self::Corners => "Corners",
            Self::Slot => "Slot",
            Self::Unframed => "Unframed",
            Self::Notch => "Notch",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Underline => "A quiet input anchored by an underline.",
            Self::Monolith => "A solid, self-contained input block.",
            Self::Lens => "A softly framed input lens.",
            Self::Signal => "An input with a whole accent-color field.",
            Self::Prompt => "A command-line-inspired input prompt.",
            Self::DotMatrix => "A dot-grid input with pixel letters.",
            Self::Corners => "An input framed by corner marks.",
            Self::Slot => "An inset input slot.",
            Self::Unframed => "An open input without an enclosing frame.",
            Self::Notch => "An input distinguished by a notch.",
        }
    }

    /// Idle geometry, independent of the configured maximum expansion height.
    pub fn bar_height(self) -> f32 {
        match self {
            Self::DotMatrix | Self::Monolith | Self::Unframed => 64.0,
            _ => 56.0,
        }
    }
}

/// Recognizes only whole, ASCII-case-insensitive `ctx` and `theme` tokens,
/// allowing `t`, `th`, `the`, and `them` while typing. Unicode whitespace is
/// accepted. `ctx themed` and other non-prefix keywords remain ordinary input.
/// Provisional `c`/`ct` input returns None: transport may withhold it separately,
/// but no theme list or theme NoMatch should appear before the `ctx` token.
///
/// `Some` always means local: bare `ctx`/theme prefixes list all designs;
/// `ctx theme <query>` filters IDs, labels and the `dot`/`dotmatrix` aliases
/// case-insensitively by substring. Unknown filters or suffixes after a partial
/// keyword yield an empty list, never remote fallthrough. No action is executed.
pub fn theme_choices(query: &str) -> Option<Vec<BarDesign>> {
    let mut tokens = query.split_whitespace();
    if !tokens.next()?.eq_ignore_ascii_case("ctx") {
        return None;
    }
    let Some(command) = tokens.next() else {
        return Some(BarDesign::ALL.to_vec());
    };
    let command = command.to_ascii_lowercase();
    if command != "theme" && !["t", "th", "the", "them"].contains(&command.as_str()) {
        return None;
    }
    let filter = tokens.collect::<Vec<_>>().join(" ").to_lowercase();
    if filter.is_empty() {
        return Some(BarDesign::ALL.to_vec());
    }
    if command != "theme" {
        return Some(vec![]);
    }
    Some(
        BarDesign::ALL
            .into_iter()
            .filter(|design| {
                design.id().contains(&filter)
                    || design.label().to_lowercase().contains(&filter)
                    || (*design == BarDesign::DotMatrix
                        && ["dot", "dotmatrix"]
                            .iter()
                            .any(|alias| alias.contains(&filter)))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ids_round_trip_and_idle_heights_are_bounded() {
        assert_eq!(BarDesign::default(), BarDesign::DotMatrix);
        let ids: std::collections::HashSet<_> =
            BarDesign::ALL.map(BarDesign::id).into_iter().collect();
        assert_eq!(ids.len(), 10);
        for design in BarDesign::ALL {
            let encoded = serde_json::to_string(&design).unwrap();
            assert_eq!(encoded, format!("\"{}\"", design.id()));
            assert_eq!(serde_json::from_str::<BarDesign>(&encoded).unwrap(), design);
            assert!(!design.label().is_empty());
            assert!(!design.description().is_empty());
            let expected = if matches!(
                design,
                BarDesign::DotMatrix | BarDesign::Monolith | BarDesign::Unframed
            ) {
                64.0
            } else {
                56.0
            };
            assert_eq!(design.bar_height(), expected);
        }
        for invalid in ["dot", "DotMatrix", "dot_matrix", "unknown", "DOT-MATRIX"] {
            assert!(serde_json::from_value::<BarDesign>(invalid.into()).is_err());
        }
    }

    #[test]
    fn bare_command_and_partial_tokens_list_all_ten() {
        for query in [
            "ctx",
            "ctx t",
            "ctx th",
            "ctx the",
            "ctx them",
            "ctx theme",
            " CTX\tThEmE\n",
            "\u{2003}ctx\u{a0}theme\u{2003}",
        ] {
            assert_eq!(
                theme_choices(query),
                Some(BarDesign::ALL.to_vec()),
                "{query:?}"
            );
        }
    }

    #[test]
    fn ids_labels_aliases_and_filters_preserve_order() {
        for design in BarDesign::ALL {
            assert_eq!(
                theme_choices(&format!("ctx theme {}", design.id())),
                Some(vec![design])
            );
            assert_eq!(
                theme_choices(&format!("ctx theme {}", design.label())),
                Some(vec![design])
            );
        }
        for query in [
            "ctx theme dot",
            "CTX THEME DOTMATRIX",
            "ctx theme dot-matrix",
            "ctx theme dot\tMatrix",
            "ctx theme Dot\u{2003}Matrix",
        ] {
            assert_eq!(theme_choices(query), Some(vec![BarDesign::DotMatrix]));
        }
        assert_eq!(
            theme_choices("ctx theme o"),
            Some(vec![
                BarDesign::Monolith,
                BarDesign::Prompt,
                BarDesign::DotMatrix,
                BarDesign::Corners,
                BarDesign::Slot,
                BarDesign::Notch
            ])
        );
    }

    #[test]
    fn keyword_boundaries_and_unicode_are_not_fuzzy_commands() {
        for query in [
            "",
            "  ",
            "change appearance",
            "c",
            "ct",
            " CT ",
            "ctxual theme",
            "ctx-theme",
            "ctx themeable",
            "ctx themed",
            "ctx themes",
            "ctx theme.dot",
            "ctx themé",
            "ctx ☃",
            "ｃｔｘ theme",
            "ctx\u{200b} theme",
            "ask ctx theme dot",
            "ctx capture",
        ] {
            assert_eq!(theme_choices(query), None, "{query:?}");
        }
    }

    #[test]
    fn reserved_unknown_filters_and_suffixes_never_fall_through() {
        for query in [
            "ctx theme unknown",
            "ctx theme dot extra",
            "ctx theme dot-matrix extra",
            "ctx theme ☃",
            "ctx theme döt",
            "ctx t dot",
            "ctx them extra",
            "ctx theme --help",
        ] {
            assert_eq!(theme_choices(query), Some(vec![]), "{query:?}");
        }
    }
}
