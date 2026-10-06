//! Pure state machine. A selection is a preview receipt or one lazy child request,
//! never an invocation. Generation and timer tickets bind each action to its list.
use ctx_bar_design::theme_choices;

use crate::model::{Fixture, Item, Platform, Presentation, Request, Suggestions};

// Hold exact provisional c/ct input locally, even across a debounce pause.
// This intentionally withholds standalone c/ct requests too. The theme parser
// still returns None for them, so the UI shows neither choices nor Theme NoMatch.
fn is_local_or_theme_prefix(query: &str) -> bool {
    let query = query.trim();
    query.eq_ignore_ascii_case("c")
        || query.eq_ignore_ascii_case("ct")
        || theme_choices(query).is_some()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Overview,
    Loading,
    Ready,
    NoMatch,
    Error(String),
    Preview(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerTicket {
    pub generation: u64,
    serial: u64,
    first_id: String,
}

#[derive(Debug, Clone)]
pub enum Selection {
    Preview(Box<Item>),
    Expand(Request),
}

/// In-memory receipt only; deliberately not persisted or sent back to the proxy.
pub struct PreviewReceipt {
    pub item: Item,
    pub request: Request,
    pub source: String,
}

pub struct State {
    pub query: String,
    pub platform: Platform,
    pub fixture: Fixture,
    pub presentation: Presentation,
    pub node: String,
    pub breadcrumb: Option<String>,
    pub generation: u64,
    pub items: Vec<Item>,
    pub selected: usize,
    pub status: Status,
    pub receipt: Option<PreviewReceipt>,
    pub source: String,
    pub latency_ms: f64,
    pub timeout_enabled: bool,
    pub timer: Option<TimerTicket>,
    pub focused: bool,
    timer_serial: u64,
}

impl State {
    pub fn new(platform: Platform, fixture: Fixture, timeout_enabled: bool) -> Self {
        Self {
            query: String::new(),
            platform,
            fixture,
            presentation: Presentation::Flat,
            node: "root".into(),
            breadcrumb: None,
            generation: 0,
            items: vec![],
            selected: 0,
            status: Status::Overview,
            receipt: None,
            source: String::new(),
            latency_ms: 0.0,
            timeout_enabled,
            timer: None,
            focused: true,
            timer_serial: 0,
        }
    }
    pub fn request(&self) -> Request {
        Request {
            query: self.query.clone(),
            platform: self.platform,
            fixture: self.fixture,
            presentation: self.presentation,
            node: self.node.clone(),
        }
    }
    /// Only loading ordinary input may be sent to the remote provider.
    /// Exact trimmed c/ct prefixes are withheld before theme parsing begins.
    /// `request` remains the unconditional snapshot API for existing callers.
    pub fn remote_request(&self) -> Option<Request> {
        if self.status != Status::Loading || is_local_or_theme_prefix(&self.query) {
            return None;
        }
        Some(self.request())
    }
    pub fn cancel_timer(&mut self) {
        self.timer = None;
        self.timer_serial += 1;
    }
    fn invalidate(&mut self) {
        self.generation += 1;
        self.cancel_timer();
        self.items.clear();
        self.receipt = None;
        self.selected = 0;
        self.source.clear();
        self.status = if self.query.trim().is_empty() || is_local_or_theme_prefix(&self.query) {
            Status::Overview
        } else {
            Status::Loading
        };
    }
    pub fn edit(&mut self, query: String) {
        self.query = query;
        self.node = "root".into();
        self.breadcrumb = None;
        self.invalidate();
        if self.query.chars().count() > 512 {
            self.status = Status::Error("Use at most 512 characters; edit to try again.".into());
        }
    }
    pub fn options(&mut self, platform: Platform, fixture: Fixture, presentation: Presentation) {
        self.platform = platform;
        self.fixture = fixture;
        self.presentation = presentation;
        self.node = "root".into();
        self.breadcrumb = None;
        self.invalidate();
    }
    pub fn back(&mut self) -> bool {
        if self.node == "root" {
            return false;
        }
        self.node = "root".into();
        self.breadcrumb = None;
        self.invalidate();
        true
    }
    pub fn escape(&mut self) {
        self.edit(String::new());
    }
    pub fn set_timeout(&mut self, enabled: bool) {
        self.timeout_enabled = enabled;
        self.cancel_timer();
        // Toggling never starts a surprise timer on an already visible list.
        // Only the next fresh response may arm it.
    }
    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.cancel_timer()
        }
    }
    pub fn complete(&mut self, generation: u64, result: Result<Suggestions, String>) -> bool {
        if generation != self.generation || self.status != Status::Loading {
            return false;
        }
        self.cancel_timer();
        match result {
            Ok(response) => {
                self.items = response.items;
                self.source = response.source;
                self.latency_ms = response.latency_ms;
                self.status = if self.items.is_empty() {
                    Status::NoMatch
                } else {
                    Status::Ready
                };
                if self.timeout_enabled && self.focused && !self.items.is_empty() {
                    self.timer = Some(TimerTicket {
                        generation,
                        serial: self.timer_serial,
                        first_id: self.items[0].id.clone(),
                    });
                }
            }
            Err(error) => self.status = Status::Error(error),
        }
        true
    }
    pub fn navigate(&mut self, delta: isize) {
        self.cancel_timer();
        if self.items.is_empty() {
            return;
        }
        self.selected =
            (self.selected as isize + delta).rem_euclid(self.items.len() as isize) as usize;
    }
    pub fn choose(&mut self, index: usize) -> Option<Selection> {
        self.cancel_timer();
        if self.status != Status::Ready {
            return None;
        }
        let item = self.items.get(index)?.clone();
        self.selected = index;
        if item.is_branch() {
            if self.presentation != Presentation::Branches || self.node != "root" {
                return None;
            }
            let node = item.node.clone()?;
            if node == "root" || node.is_empty() {
                return None;
            }
            self.node = node;
            self.breadcrumb = Some(item.label.clone());
            self.invalidate();
            Some(Selection::Expand(self.request()))
        } else {
            self.receipt = Some(PreviewReceipt {
                item: item.clone(),
                request: self.request(),
                source: self.source.clone(),
            });
            self.status = Status::Preview(format!(
                "Selected {} / {}: {} — preview only; nothing executed.",
                item.tool, item.interface, item.label
            ));
            self.generation += 1; // A preview cannot be selected twice by an old callback.
            Some(Selection::Preview(Box::new(item)))
        }
    }
    pub fn timer_fire(&mut self, ticket: &TimerTicket) -> Option<Selection> {
        if !self.focused
            || !self.timeout_enabled
            || self.timer.as_ref() != Some(ticket)
            || self.generation != ticket.generation
            || self.items.first().map(|i| &i.id) != Some(&ticket.first_id)
        {
            return None;
        }
        self.choose(0)
    }
}

#[cfg(test)]
#[path = "state_theme_tests.rs"]
mod theme_tests;

// Suggestion lifecycle fixtures are shared with the local-command boundary
// suite; preview/branch/timer behavior remains covered here.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixture_catalog;
    pub(super) fn response() -> Suggestions {
        let mut items: Vec<_> = fixture_catalog()
            .unwrap()
            .items
            .into_iter()
            .take(2)
            .collect();
        for item in &mut items {
            item.probability = Some(0.5)
        }
        Suggestions {
            items,
            source: "Jev via EAVS".into(),
            latency_ms: 25.0,
        }
    }
    pub(super) fn ready(enabled: bool) -> State {
        let mut state = State::new(Platform::Macos, Fixture::Desktop, enabled);
        state.edit("change appearance".into());
        state.complete(state.generation, Ok(response()));
        state
    }
    #[test]
    fn flat_and_timeout_off_are_baseline() {
        let state = ready(false);
        assert_eq!(state.presentation, Presentation::Flat);
        assert!(state.timer.is_none());
    }
    #[test]
    fn only_current_generation_can_land() {
        let mut s = ready(true);
        let old = s.generation;
        s.edit("other query".into());
        assert!(!s.complete(old, Ok(response())));
        assert!(s.items.is_empty());
        assert!(!s.complete(old, Err("stale error".into())));
        assert_eq!(s.status, Status::Loading);
    }
    #[test]
    fn first_item_timeout_is_single_shot_not_confidence_policy() {
        let mut s = ready(true);
        let ticket = s.timer.clone().unwrap();
        let first = s.items[0].id.clone();
        let Some(Selection::Preview(item)) = s.timer_fire(&ticket) else {
            panic!("expected preview")
        };
        assert_eq!(item.id, first);
        assert!(s.timer_fire(&ticket).is_none());
    }
    #[test]
    fn edits_navigation_escape_options_focus_and_disable_cancel_timer() {
        for cancel in 0..6 {
            let mut s = ready(true);
            let ticket = s.timer.clone().unwrap();
            match cancel {
                0 => s.edit("new".into()),
                1 => s.navigate(1),
                2 => s.escape(),
                3 => s.options(Platform::Linux, Fixture::Audio, Presentation::Branches),
                4 => s.focus(false),
                _ => s.set_timeout(false),
            }
            assert!(s.timer_fire(&ticket).is_none());
        }
    }
    #[test]
    fn no_match_error_and_manual_choice_never_execute() {
        let mut s = ready(false);
        s.navigate(1);
        let expected = s.items[1].id.clone();
        let Some(Selection::Preview(item)) = s.choose(s.selected) else {
            panic!("expected preview")
        };
        assert_eq!(item.id, expected);
        let receipt = s.receipt.as_ref().unwrap();
        assert_eq!(receipt.item.id, expected);
        assert_eq!(receipt.request.fixture, Fixture::Desktop);
        assert_eq!(receipt.source, "Jev via EAVS");
        assert!(s.choose(1).is_none());
        s.edit("unmatched".into());
        let mut r = response();
        r.items.clear();
        s.complete(s.generation, Ok(r));
        assert_eq!(s.status, Status::NoMatch);
        assert!(s.timer.is_none());
        s.edit("retry".into());
        s.complete(s.generation, Err("Proxy unavailable".into()));
        assert!(matches!(s.status, Status::Error(_)));
        assert!(s.items.is_empty());
    }
    #[test]
    fn lazy_branch_is_one_request_and_back_discards_child_results() {
        let mut s = ready(true);
        s.presentation = Presentation::Branches;
        s.items[0].kind = Some("branch".into());
        s.items[0].node = Some("fixture-branch".into());
        let ticket = s.timer.clone().unwrap();
        let Some(Selection::Expand(request)) = s.timer_fire(&ticket) else {
            panic!("expected one child request")
        };
        assert_eq!(request.node, "fixture-branch");
        assert_eq!(request.query, "change appearance");
        assert!(s.choose(0).is_none());
        assert!(s.timer_fire(&ticket).is_none());
        let child_generation = s.generation;
        assert!(s.back());
        assert!(!s.complete(child_generation, Ok(response())));
        assert_eq!(s.node, "root");
    }
    #[test]
    fn changes_inside_child_reset_root_and_depth_is_two() {
        let mut s = ready(false);
        s.presentation = Presentation::Branches;
        s.items[0].kind = Some("branch".into());
        s.items[0].node = Some("fixture-branch".into());
        s.choose(0);
        let mut r = response();
        r.items[0].kind = Some("branch".into());
        r.items[0].node = Some("too-deep".into());
        s.complete(s.generation, Ok(r));
        assert!(s.choose(0).is_none());
        s.edit("new".into());
        assert_eq!(s.node, "root");
        assert!(s.breadcrumb.is_none());
    }
}
