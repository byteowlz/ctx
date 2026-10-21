//! Local presentation controls. Parsing does not infer, persist, or execute.
use ctx_bar_design::{BarDesign, theme_choices};

use crate::model::{TreeVariant, ViewMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Design(BarDesign),
    View(ViewMode),
    Variant(TreeVariant),
    Dwell(bool),
    /// Start in-app decisions setup (endpoint, key, test call).
    Setup,
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
            Self::Setup => "Set up Jev endpoint",
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
            Self::Setup => "Endpoint URL and API key, tested before saving. Stored owner-only.",
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
            Self::Setup => "jev decisions endpoint key",
            Self::Design(_) => "",
        };
        aliases
            .split_whitespace()
            .any(|alias| alias.starts_with(filter))
    }
}

/// Plain words that name bar commands. Each needs at least four typed
/// characters, and a test guards that no catalog word shares these stems, so
/// a bar command can never shadow a real catalog match.
const COMMAND_WORDS: [(&str, &[Action]); 11] = [
    (
        "tree",
        &[
            Action::View(ViewMode::Tree),
            Action::Variant(TreeVariant::Faint),
            Action::Variant(TreeVariant::Trail),
            Action::Variant(TreeVariant::Crumbs),
        ],
    ),
    ("branches", &[Action::View(ViewMode::Tree)]),
    ("list", &[Action::View(ViewMode::List)]),
    ("classic", &[Action::View(ViewMode::List)]),
    ("flat", &[Action::View(ViewMode::List)]),
    ("faint", &[Action::Variant(TreeVariant::Faint)]),
    ("trail", &[Action::Variant(TreeVariant::Trail)]),
    ("breadcrumbs", &[Action::Variant(TreeVariant::Crumbs)]),
    ("crumbs", &[Action::Variant(TreeVariant::Crumbs)]),
    ("dwell", &[Action::Dwell(true), Action::Dwell(false)]),
    ("setup", &[Action::Setup]),
];
/// Words that may surround a command word ("show tree", "switch to list").
/// Catalog words such as "mode" or "layout" are deliberately excluded.
const FILLER: [&str; 6] = ["show", "switch", "use", "to", "the", "view"];

/// Bar commands for a plain query, without the `ctx` prefix. Every token must
/// be a filler word or a 4+ character prefix of a command word; a lone "view"
/// offers both views. Returns None for anything else, so ordinary requests
/// still reach catalog matching and Jev.
pub fn suggest(query: &str) -> Option<Vec<Action>> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let mut actions: Vec<Action> = vec![];
    for token in &tokens {
        let token = token.trim_end_matches('s');
        if FILLER.contains(&token)
            || FILLER
                .iter()
                .any(|filler| filler.trim_end_matches('s') == token)
        {
            continue;
        }
        if token.len() < 4 {
            return None;
        }
        let matched: Vec<Action> = COMMAND_WORDS
            .iter()
            .filter(|(word, _)| word.starts_with(token))
            .flat_map(|(_, found)| found.iter().copied())
            .collect();
        if matched.is_empty() {
            return None;
        }
        for action in matched {
            if !actions.contains(&action) {
                actions.push(action);
            }
        }
    }
    if actions.is_empty()
        && tokens
            .iter()
            .any(|token| token.trim_end_matches('s') == "view")
    {
        actions = vec![Action::View(ViewMode::List), Action::View(ViewMode::Tree)];
    }
    (!actions.is_empty()).then_some(actions)
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
        ("ctx setup", vec![Action::Setup]),
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
    if query.starts_with("ctx ") || query == "ctx" {
        return None;
    }
    suggest(&query)
}

pub fn holds_input(query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query == "c"
        || query == "ct"
        || choices(&query).is_some()
        || ["ctx view", "ctx tree", "ctx dwell", "ctx setup"]
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

    #[test]
    fn plain_command_words_open_the_local_menu() {
        let tree = suggest("tree").unwrap();
        assert_eq!(tree[0], Action::View(ViewMode::Tree));
        assert_eq!(tree.len(), 4, "tree view plus its three path variants");
        assert_eq!(suggest("Show Tree"), Some(tree.clone()));
        assert_eq!(
            suggest("switch to list"),
            Some(vec![Action::View(ViewMode::List)])
        );
        assert_eq!(
            suggest("breadcrumbs"),
            Some(vec![Action::Variant(TreeVariant::Crumbs)])
        );
        assert_eq!(
            suggest("view"),
            Some(vec![
                Action::View(ViewMode::List),
                Action::View(ViewMode::Tree)
            ])
        );
        assert!(holds_input("tree"), "command words are not sent to Jev");
        assert_eq!(choices("tree"), Some(tree));
    }

    #[test]
    fn ordinary_requests_are_never_mistaken_for_commands() {
        for query in [
            "tr",
            "tre",
            "t",
            "switch to dark mode",
            "mode",
            "keyboard layout",
            "adjust volume",
            "tree house",
            "ctx tree house",
            "lis",
            "dwel x",
        ] {
            assert_eq!(suggest(query), None, "{query}");
        }
        for query in ["switch to dark mode", "adjust volume"] {
            assert!(!holds_input(query), "{query} must still reach Jev");
        }
    }

    #[test]
    fn command_words_never_shadow_catalog_words() {
        let catalog = crate::model::embedded_catalog().expect("embedded catalog");
        let words: Vec<String> = catalog
            .items
            .iter()
            .chain(&catalog.branches)
            .flat_map(|item| {
                format!(
                    "{} {} {} {}",
                    item.label, item.tool, item.interface, item.id
                )
                .to_ascii_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
            })
            .collect();
        for (command, _) in COMMAND_WORDS {
            let stem = &command[..4];
            let clash: Vec<_> = words.iter().filter(|word| word.starts_with(stem)).collect();
            assert!(
                clash.is_empty(),
                "command word {command} shadows catalog words {clash:?}"
            );
        }
        for filler in FILLER {
            if ["show", "switch", "use", "to", "the"].contains(&filler) {
                continue; // connective words; never sole tokens of a command
            }
            assert!(
                !words.iter().any(|word| word == filler),
                "filler {filler} is a catalog word"
            );
        }
    }
}
