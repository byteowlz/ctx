use crate::model::{Presentation, Suggestions, TreeVariant, ViewMode, fixture_catalog};
use crate::state::{Selection, State, Status};
use crate::tree::{Motion, Plan, Position, visible_range};

pub(super) fn ready_tree() -> State {
    let catalog = fixture_catalog().unwrap();
    let mut state = State::new(
        crate::model::Platform::Macos,
        crate::model::Fixture::Desktop,
        true,
    );
    state.switch_view(ViewMode::Tree);
    state.edit("change appearance".into());
    let mut items = catalog.branches.clone();
    for item in &mut items {
        item.probability = Some(1.0 / catalog.branches.len() as f64);
    }
    state.complete(
        state.generation,
        Ok(Suggestions {
            items,
            source: "Jev via EAVS".into(),
            latency_ms: 1.0,
        }),
    );
    state
}

#[test]
fn native_tree_and_list_share_routing_but_switch_invalidates_scope_and_tickets() {
    let mut s = ready_tree();
    let old = s.generation;
    s.switch_view(ViewMode::List);
    assert_eq!(s.query, "change appearance");
    assert_eq!(s.presentation, Presentation::Flat);
    assert_eq!(s.node, "root");
    assert!(s.items.is_empty());
    assert!(!s.settle_tree(old));
    assert!(!s.resume_dwell());
}
#[test]
fn branch_preview_and_back_restore_scopes_and_selection_without_execution() {
    let mut s = ready_tree();
    let parent_ids: Vec<_> = s.items.iter().map(|i| i.id.clone()).collect();
    assert!(matches!(s.choose(0), Some(Selection::Expand(_))));
    let child_generation = s.generation;
    let catalog = fixture_catalog().unwrap();
    let items = catalog
        .candidates(&s.request())
        .unwrap()
        .into_iter()
        .cloned()
        .collect();
    s.complete(
        s.generation,
        Ok(Suggestions {
            items,
            source: "Jev via EAVS".into(),
            latency_ms: 2.0,
        }),
    );
    assert!(matches!(s.choose(1), Some(Selection::Preview(_))));
    assert!(matches!(s.status, Status::Preview(_)));
    assert!(s.timer.is_none());
    assert!(s.back());
    assert_eq!(s.selected, 1);
    assert_eq!(s.node, "fixture-appearance");
    assert!(s.receipt.is_none());
    assert!(s.back());
    assert_eq!(s.node, "root");
    assert_eq!(
        s.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
        parent_ids
    );
    assert!(!s.complete(child_generation, Err("stale".into())));
}
#[test]
fn dwell_requires_settlement_and_explicit_resume_after_intervention() {
    let mut s = ready_tree();
    assert!(s.timer.is_none());
    assert!(!s.resume_dwell());
    s.settle_tree(s.generation);
    assert!(s.resume_dwell());
    let ticket = s.timer.clone().unwrap();
    s.navigate(1);
    assert!(s.timer_fire(&ticket).is_none());
    assert!(!s.resume_dwell());
    s.settle_tree(s.generation);
    assert!(s.resume_dwell());
    s.focus(false);
    s.focus(true);
    assert!(s.timer.is_none());
    s.settle_tree(s.generation);
    assert!(s.resume_dwell());
    s.edit("other".into());
    assert!(!s.resume_dwell());
}
#[test]
fn local_catalog_and_refusal_never_arm_tree_dwell() {
    let mut s = ready_tree();
    let items = s.items.clone();
    s.browse(items);
    s.settle_tree(s.generation);
    assert!(!s.resume_dwell());
    assert!(s.timer.is_none());
    s.edit("unmatched".into());
    s.complete(
        s.generation,
        Ok(Suggestions {
            items: vec![],
            source: "Jev via EAVS".into(),
            latency_ms: 1.0,
        }),
    );
    s.settle_tree(s.generation);
    assert_eq!(s.status, Status::NoMatch);
    assert!(!s.resume_dwell());
}
#[test]
fn every_variant_keeps_selected_center_and_full_catalog_indices() {
    let mut s = ready_tree();
    s.items = fixture_catalog().unwrap().items;
    for variant in TreeVariant::ALL {
        for width in [480.0, 680.0, 1200.0] {
            for selected in [0, 7, s.items.len() - 1] {
                s.selected = selected;
                let p = Plan::new(&s, *variant, width, 320.0);
                assert_eq!(p.selected_center(), Some(p.center));
                assert!(p.nodes.iter().filter(|n| !n.prior).count() <= 5);
                assert!(p.nodes.iter().any(|n| n.index == Some(selected)));
            }
        }
    }
    assert_eq!(visible_range(0, 0), 0..0);
}
#[test]
fn interrupted_motion_retargets_current_position_and_reduced_motion_finishes() {
    let mut m = Motion::default();
    let t = std::time::Instant::now();
    m.retarget(Position { x: 10.0, y: 20.0 }, t, false);
    m.retarget(Position { x: 100.0, y: 200.0 }, t, false);
    let halfway = t + std::time::Duration::from_millis(110);
    let from = m.position(halfway);
    m.retarget(Position { x: 0.0, y: 0.0 }, halfway, false);
    assert_eq!(m.position(halfway), from);
    m.retarget(Position { x: 1.0, y: 2.0 }, halfway, true);
    assert!(m.settled(halfway));
    assert_eq!(m.position(halfway), Position { x: 1.0, y: 2.0 });
}
