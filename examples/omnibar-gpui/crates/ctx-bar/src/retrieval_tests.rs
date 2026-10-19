use crate::model::{
    Fixture, LocalSuggestions, Platform, Presentation, Suggestions, fixture_catalog, validate_local,
};
use crate::state::{Selection, State, Status};

fn state() -> State {
    let mut state = State::new(Platform::Macos, Fixture::Desktop, true);
    state.edit("light mode".into());
    state
}
fn local(state: &State) -> Vec<crate::model::Item> {
    fixture_catalog()
        .unwrap()
        .candidates(&state.request())
        .unwrap()
        .into_iter()
        .take(5)
        .cloned()
        .collect()
}
fn response(ids: Vec<String>) -> LocalSuggestions {
    LocalSuggestions {
        ids,
        source: "Local Evoke".into(),
        latency_ms: 3.0,
        truncated: false,
    }
}
#[test]
fn provisional_keeps_jev_pending_and_never_arms_timer() {
    let mut state = state();
    let items = local(&state);
    assert!(state.complete_local(state.generation, Ok(items)));
    assert_eq!(state.status, Status::Loading);
    assert!(state.remote_request().is_some());
    assert!(state.timer.is_none());
    assert!(state.items.iter().all(|item| item.probability.is_none()));
    assert!(matches!(state.choose(0), Some(Selection::Preview(_))));
    assert_eq!(state.receipt.as_ref().unwrap().source, "Local Evoke");
    assert!(state.local_note.is_none());
}
#[test]
fn refusal_and_failure_clear_provisional_rows_without_inventing_decisions() {
    for decision in [
        Ok(Suggestions {
            items: vec![],
            source: "Jev via EAVS".into(),
            latency_ms: 4.0,
        }),
        Err("Synthetic unavailable".into()),
    ] {
        let mut state = state();
        assert!(state.complete_local(state.generation, Ok(local(&state))));
        assert!(state.complete(state.generation, decision));
        assert!(state.items.is_empty());
        assert!(state.timer.is_none());
        assert!(!state.provisional);
        assert!(state.choose(0).is_none());
        assert!(!state.complete_local(state.generation, Ok(local(&state))));
    }
}
#[test]
fn old_local_and_remote_results_cannot_replace_edits_escape_preview_or_all_catalog() {
    let mut state = state();
    let generation = state.generation;
    state.edit("audio".into());
    assert!(!state.complete_local(generation, Ok(local(&state))));
    let generation = state.generation;
    assert!(
        state.browse(
            fixture_catalog()
                .unwrap()
                .candidates(&state.request())
                .unwrap()
                .into_iter()
                .cloned()
                .collect()
        )
    );
    assert!(state.items.len() > 5);
    assert!(state.timer.is_none());
    assert_eq!(state.source, "Catalog browse");
    assert!(state.remote_request().is_none());
    assert!(!state.complete(generation, Err("Late result".into())));
    let old_id = state.items[0].id.clone();
    assert!(state.choose_current(generation, 0, &old_id).is_none());
    let current = state.generation;
    assert!(
        state
            .choose_current(current, 0, "wrong-row-after-replacement")
            .is_none()
    );
    assert!(state.choose_current(current, 0, &old_id).is_some());
    assert_eq!(state.receipt.as_ref().unwrap().source, "Catalog browse");
    assert!(!state.complete_local(current, Ok(local(&state))));
    state.escape();
    assert!(state.items.is_empty());
}
#[test]
fn intervention_preserves_selected_identity_and_does_not_rearm() {
    let mut state = state();
    let mut items = local(&state);
    state.complete_local(state.generation, Ok(items.clone()));
    state.navigate(1);
    let selected = state.items[1].id.clone();
    items.reverse();
    state.complete(
        state.generation,
        Ok(Suggestions {
            items,
            source: "Jev via EAVS".into(),
            latency_ms: 2.0,
        }),
    );
    assert_eq!(state.items[state.selected].id, selected);
    assert!(state.timer.is_none());
}
#[test]
fn redundant_native_submit_change_does_not_reopen_catalog_preview() {
    let mut state = state();
    state.browse(local(&state));
    state.choose(0);
    let generation = state.generation;
    assert!(!state.input_change(state.query.clone()));
    assert_eq!(state.generation, generation);
    assert!(matches!(state.status, Status::Preview(_)));
    assert_eq!(state.receipt.as_ref().unwrap().source, "Catalog browse");
    assert!(state.input_change("new actual input".into()));
    assert!(state.receipt.is_none());
    assert_eq!(state.status, Status::Loading);
}

#[test]
fn local_commands_and_empty_input_bypass_both_inference_and_browse() {
    for query in ["", "c", "ct", "ctx theme", "ctx theme dot"] {
        let mut state = state();
        state.edit(query.into());
        assert!(state.remote_request().is_none());
        assert!(!state.complete_local(state.generation, Ok(vec![])));
        assert!(!state.browse(vec![]));
    }
}
#[test]
fn uncalibrated_local_contract_rejects_wrong_scope_ids_duplicates_and_invalid_envelopes() {
    let mut state = state();
    let catalog = fixture_catalog().unwrap();
    let valid = local(&state)[0].id.clone();
    assert!(validate_local(response(vec![valid.clone()]), &state.request(), &catalog).is_ok());
    for ids in [
        vec!["unknown".into()],
        vec![valid.clone(), valid.clone()],
        vec![valid.clone(); 6],
    ] {
        assert!(validate_local(response(ids), &state.request(), &catalog).is_err());
    }
    for latency in [f64::NAN, f64::INFINITY, -1.0] {
        let mut invalid = response(vec![]);
        invalid.latency_ms = latency;
        assert!(validate_local(invalid, &state.request(), &catalog).is_err());
    }
    let mut invalid = response(vec![]);
    invalid.truncated = true;
    assert!(validate_local(invalid, &state.request(), &catalog).is_err());
    let mut invalid = response(vec![]);
    invalid.source = "Jev via EAVS".into();
    assert!(validate_local(invalid, &state.request(), &catalog).is_err());
    state.presentation = Presentation::Branches;
    state.node = "fixture-sound".into();
    assert!(validate_local(response(vec![valid]), &state.request(), &catalog).is_err());
    for platform in Platform::ALL {
        for fixture in Fixture::ALL {
            for presentation in Presentation::ALL {
                state.options(*platform, *fixture, *presentation);
                let offered = catalog.candidates(&state.request()).unwrap();
                let ids = offered.iter().take(5).map(|item| item.id.clone()).collect();
                let result = validate_local(response(ids), &state.request(), &catalog).unwrap();
                assert!(result.iter().all(|item| item.offered(*platform, *fixture) && item.probability.is_none()));
            }
        }
    }
}
#[test]
fn local_branches_request_one_declared_child_and_invalidate_root_work() {
    let mut state = state();
    state.presentation = Presentation::Branches;
    let catalog = fixture_catalog().unwrap();
    let branch = catalog
        .candidates(&state.request())
        .unwrap()
        .into_iter()
        .find(|item| item.is_branch())
        .unwrap()
        .clone();
    let generation = state.generation;
    state.complete_local(generation, Ok(vec![branch]));
    assert!(matches!(state.choose(0), Some(Selection::Expand(_))));
    assert_ne!(state.node, "root");
    assert!(!state.complete_local(generation, Ok(vec![])));
    assert!(
        catalog
            .candidates(&state.request())
            .unwrap()
            .iter()
            .all(|item| !item.is_branch())
    );
    assert!(state.back());
    assert_eq!(state.node, "root");
}

#[test]
fn offline_fuzzy_survives_a_transport_error_and_never_arms_a_timer() {
    let catalog = crate::model::fixture_catalog().unwrap();
    let mut state = crate::state::State::new(
        crate::model::Platform::Macos,
        crate::model::Fixture::Desktop,
        true,
    );
    state.edit("volum".into());
    let generation = state.generation;
    // The adapter is down: the transport fails first.
    assert!(state.complete(generation, Err("Start the local prototype proxy".into())));
    assert!(matches!(state.status, crate::state::Status::Error(_)));
    let offered = catalog.candidates(&state.request()).unwrap();
    let ranked = crate::fuzzy::rank(
        &state.query,
        &offered
            .iter()
            .map(|item| (*item).clone())
            .collect::<Vec<_>>(),
        5,
    );
    assert!(!ranked.is_empty(), "fuzzy must match the offered scope");
    let picked: Vec<_> = ranked
        .into_iter()
        .map(|index| offered[index].clone())
        .collect();
    assert!(state.show_local(
        generation,
        picked,
        "Local fuzzy",
        "Adapter unavailable · local matches · F2: all".into(),
        true
    ));
    assert_eq!(
        state.status,
        crate::state::Status::Overview,
        "offline matches settle the spinner"
    );
    assert!(state.provisional);
    assert!(state.timer.is_none(), "local matches never arm a timer");
    assert!(
        state
            .local_note
            .as_deref()
            .unwrap_or_default()
            .contains("Adapter unavailable")
    );
    assert!(state.items.iter().all(|item| item.probability.is_none()));
}
