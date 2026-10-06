//! Local theme transport boundary: provisional input, ordinary requests,
//! timer cancellation, and pending-completion safety share lifecycle fixtures.
use super::tests::{ready, response};
use super::*;

#[test]
fn remote_requests_require_loading_ordinary_input() {
    let mut s = State::new(Platform::Macos, Fixture::Desktop, false);
    assert!(s.remote_request().is_none());
    s.edit("ctx themed".into());
    let request = s.remote_request().unwrap();
    assert_eq!(request.query, s.request().query);
    assert_eq!(request.platform, s.request().platform);
    assert_eq!(request.fixture, s.request().fixture);
    assert_eq!(request.presentation, s.request().presentation);
    assert_eq!(request.node, s.request().node);
    for status in [
        Status::Overview,
        Status::Ready,
        Status::NoMatch,
        Status::Error("error".into()),
        Status::Preview("preview".into()),
    ] {
        s.status = status;
        assert!(s.remote_request().is_none());
    }
    s.edit("x".repeat(513));
    assert!(s.remote_request().is_none());
    s.edit("normal request".into());
    assert!(s.remote_request().is_some());
    assert!(s.complete(s.generation, Ok(response())));
    assert!(s.remote_request().is_none());
}
#[test]
fn provisional_prefixes_hide_theme_results_and_release_ordinary_input() {
    let mut s = ready(true);
    for query in ["c", "ct", " C ", "\u{2003}cT\u{a0}"] {
        assert_eq!(theme_choices(query), None);
        s.edit(query.into());
        assert_eq!(s.status, Status::Overview);
        assert!(s.timer.is_none());
        assert!(s.remote_request().is_none());
    }
    for query in [
        "ctx capture",
        "ctx themed",
        "c request",
        "ct scan",
        "cat",
        "ｃ",
        "c\u{200b}",
    ] {
        assert_eq!(theme_choices(query), None);
        s.edit(query.into());
        assert_eq!(s.status, Status::Loading);
        assert_eq!(s.remote_request().unwrap().query, query);
    }
}
#[test]
fn local_commands_cancel_timers_and_reject_pending_completions() {
    for query in [
        "c",
        "ct",
        " C ",
        "\u{2003}cT\u{a0}",
        "ctx",
        "ctx t",
        "ctx theme",
        "ctx theme dot",
        "ctx theme unknown",
        "ctx theme dot extra",
        "CTX\tTHEME\u{2003}DOT",
    ] {
        let mut s = ready(true);
        let ticket = s.timer.clone().unwrap();
        s.edit("pending request".into());
        let pending_generation = s.generation;
        s.edit(query.into());
        assert_eq!(s.status, Status::Overview);
        assert!(s.remote_request().is_none());
        assert_eq!(s.request().query, query); // Snapshot contract is unchanged.
        assert!(s.items.is_empty());
        assert!(s.receipt.is_none());
        assert!(s.timer_fire(&ticket).is_none());
        assert!(s.choose(0).is_none());
        assert!(!s.complete(pending_generation, Ok(response())));
        assert!(!s.complete(s.generation, Ok(response())));
        assert!(!s.complete(s.generation, Err("pending error".into())));
        s.options(Platform::Linux, Fixture::Audio, Presentation::Branches);
        assert_eq!(s.status, Status::Overview);
        assert!(!s.complete(s.generation, Ok(response())));
        // Defense in depth: even a caller forcing Loading cannot route local input.
        s.status = Status::Loading;
        assert!(s.remote_request().is_none());
        s.escape();
        assert!(s.remote_request().is_none());
        s.edit("resume Jev suggestions".into());
        assert!(s.remote_request().is_some());
    }
}
