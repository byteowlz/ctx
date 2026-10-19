//! Local presentation controls. Parsing does not infer, persist, or execute.
use ctx_bar_design::{BarDesign, theme_choices};

use crate::model::{TreeVariant, ViewMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Design(BarDesign),
    View(ViewMode),
    Variant(TreeVariant),
    Dwell(bool),
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Design(design) => design.label(),
            Self::View(ViewMode::List) => "Classic list",
            Self::View(ViewMode::Tree) => "Moving branches",
            Self::Variant(variant) => variant.label(),
            Self::Dwell(true) => "Enable preview dwell",
            Self::Dwell(false) => "Disable preview dwell",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::Design(design) => design.description(),
            Self::View(ViewMode::List) => "Compact rows. Switch views with the ctx view command.",
            Self::View(ViewMode::Tree) => {
                "Centered choices, moving path, reversible Back. Preview only."
            }
            Self::Variant(TreeVariant::Faint) => "Keep earlier choices faint on the left.",
            Self::Variant(TreeVariant::Trail) => "Keep only the chosen horizontal path.",
            Self::Variant(TreeVariant::Crumbs) => {
                "Hide earlier nodes; retain a compact breadcrumb."
            }
            Self::Dwell(true) => "Opt-in 2s tree dwell after settlement. Explicit Resume required.",
            Self::Dwell(false) => "Only manual navigation; no automatic selection.",
        }
    }
    fn matches(self, filter: &str) -> bool {
        let aliases = match self {
            Self::View(ViewMode::List) => "list classic flat",
            Self::View(ViewMode::Tree) => "tree branches branch",
            Self::Variant(TreeVariant::Faint) => "faint",
            Self::Variant(TreeVariant::Trail) => "trail",
            Self::Variant(TreeVariant::Crumbs) => "crumbs breadcrumbs",
            Self::Dwell(true) => "on enable",
            Self::Dwell(false) => "off disable",
            Self::Design(_) => "",
        };
        aliases
            .split_whitespace()
            .any(|alias| alias.starts_with(filter))
    }
}

pub fn choices(query: &str) -> Option<Vec<Action>> {
    if let Some(designs) = theme_choices(query) {
        return Some(designs.into_iter().map(Action::Design).collect());
    }
    let query = query.trim().to_ascii_lowercase();
    for (prefix, options) in [
        (
            "ctx view",
            vec![Action::View(ViewMode::List), Action::View(ViewMode::Tree)],
        ),
        (
            "ctx tree",
            TreeVariant::ALL
                .iter()
                .copied()
                .map(Action::Variant)
                .collect(),
        ),
        ("ctx dwell", vec![Action::Dwell(false), Action::Dwell(true)]),
    ] {
        if query == prefix {
            return Some(options);
        }
        if let Some(filter) = query.strip_prefix(&format!("{prefix} ")) {
            return Some(
                options
                    .into_iter()
                    .filter(|action| action.matches(filter))
                    .collect(),
            );
        }
    }
    None
}

pub fn holds_input(query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query == "c"
        || query == "ct"
        || choices(&query).is_some()
        || ["ctx view", "ctx tree", "ctx dwell"]
            .iter()
            .any(|prefix| query.starts_with("ctx ") && prefix.starts_with(&query))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_choices_and_unknown_suffixes_never_route() {
        assert_eq!(
            choices("CTX view branch"),
            Some(vec![Action::View(ViewMode::Tree)])
        );
        assert_eq!(
            choices("ctx tree trail"),
            Some(vec![Action::Variant(TreeVariant::Trail)])
        );
        assert_eq!(choices("ctx dwell on"), Some(vec![Action::Dwell(true)]));
        assert_eq!(choices("ctx tree impossible"), Some(vec![]));
        for query in ["ctx v", "ctx tr", "ctx dwell", "ctx theme dot"] {
            assert!(holds_input(query));
        }
        for query in ["ctx capture", "ctx themed", "ctx views", "cat", "ct scan"] {
            assert!(!holds_input(query));
        }
    }
}
