use crate::model::{Presentation, Suggestions, TreeVariant, ViewMode, fixture_catalog};
use crate::state::{Selection, Status};
use crate::tree::Plan;
use crate::tree_tests::ready_tree;

#[test]
fn automatic_branch_waits_for_fresh_jev_and_settlement_then_stops_at_preview() {
    let mut s = ready_tree();
    s.settle_tree(s.generation);
    assert!(s.resume_dwell());
    assert!(matches!(
        s.timer_fire(&s.timer.clone().unwrap()),
        Some(Selection::Expand(_))
    ));
    let catalog = fixture_catalog().unwrap();
    let mut items: Vec<_> = catalog
        .candidates(&s.request())
        .unwrap()
        .into_iter()
        .take(5)
        .cloned()
        .collect();
    assert!(s.complete_local(s.generation, Ok(items.clone())));
    s.settle_tree(s.generation);
    assert!(s.timer.is_none());
    for item in &mut items {
        item.probability = Some(0.2);
    }
    assert!(s.complete(
        s.generation,
        Ok(Suggestions {
            items,
            source: "Jev via EAVS".into(),
            latency_ms: 1.0
        })
    ));
    assert!(s.timer.is_none());
    s.settle_tree(s.generation);
    let ticket = s.timer.clone().unwrap();
    assert!(matches!(s.timer_fire(&ticket), Some(Selection::Preview(_))));
    assert!(s.timer.is_none());
    assert!(!s.resume_dwell());
    assert!(s.timer_fire(&ticket).is_none());
}
#[test]
fn mode_switch_preserves_final_receipt_not_an_incompatible_child_request() {
    let mut s = ready_tree();
    s.choose(0);
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
            latency_ms: 1.0,
        }),
    );
    s.choose(0);
    let original = serde_json::to_value(&s.receipt.as_ref().unwrap().request).unwrap();
    s.switch_view(ViewMode::List);
    assert!(matches!(s.status, Status::Preview(_)));
    assert_eq!(
        serde_json::to_value(&s.receipt.as_ref().unwrap().request).unwrap(),
        original
    );
    assert!(s.timer.is_none());
    assert!(s.back());
    assert_eq!(s.node, "root");
    assert_eq!(s.presentation, Presentation::Flat);
    assert_eq!(s.status, Status::Loading);
    assert!(s.receipt.is_none());
}
#[test]
fn every_child_and_final_variant_has_one_center_with_no_recursive_choices() {
    let mut s = ready_tree();
    s.choose(0);
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
            latency_ms: 1.0,
        }),
    );
    for variant in TreeVariant::ALL {
        for width in [480.0, 680.0, 1200.0] {
            for selected in 0..s.items.len() {
                s.selected = selected;
                let p = Plan::new(&s, *variant, width, 320.0);
                let center = p.selected_center().unwrap();
                assert!((center.x - p.center.x).abs() < 0.001);
                assert!((center.y - p.center.y).abs() < 0.001);
                assert!(p.nodes.iter().filter(|n| !n.prior).count() <= 5);
                assert_eq!(
                    p.nodes.iter().filter(|n| n.prior).count() == 0,
                    *variant == TreeVariant::Crumbs
                );
            }
        }
    }
    s.choose(0);
    for variant in TreeVariant::ALL {
        let p = Plan::new(&s, *variant, 680.0, 320.0);
        assert_eq!(p.nodes.iter().filter(|n| !n.prior).count(), 1);
        assert!(
            p.nodes
                .iter()
                .filter(|n| !n.prior)
                .all(|n| n.index.is_none())
        );
    }
}
#[test]
fn synthetic_or_uncalibrated_ready_data_cannot_resume_tree_dwell() {
    let mut s = ready_tree();
    s.source = "synthetic-native-review-not-Jev".into();
    s.settle_tree(s.generation);
    assert!(!s.resume_dwell());
    s.source = "Jev via EAVS".into();
    for item in &mut s.items {
        item.probability = None;
    }
    assert!(!s.resume_dwell());
}
#[test]
fn unverified_platform_conditions_are_metadata_not_executable_or_forgeable() {
    let catalog = fixture_catalog().unwrap();
    let mut s = ready_tree();
    s.switch_view(ViewMode::List);
    let mut item = catalog
        .items
        .iter()
        .find(|item| item.id == "sound.apps")
        .unwrap()
        .clone();
    assert!(
        item.support_for(s.platform)
            .unwrap()
            .contains("no standard native mixer")
    );
    item.probability = Some(1.0);
    let mut response = Suggestions {
        items: vec![item],
        source: "Jev via EAVS".into(),
        latency_ms: 1.0,
    };
    assert!(crate::model::validate(&response, &s.request(), &catalog).is_ok());
    response.items[0]
        .support
        .insert("macos".into(), "Always available; granted".into());
    assert!(crate::model::validate(&response, &s.request(), &catalog).is_err());
    let mut json = serde_json::to_value(&catalog.items[0]).unwrap();
    json["exec"] = serde_json::json!("not-an-invocation-contract");
    assert!(serde_json::from_value::<crate::model::Item>(json).is_err());
}
#[test]
fn tree_geometry_is_fixed_so_the_aperture_never_moves() {
    for design in ctx_bar_design::BarDesign::ALL {
        let geometry = crate::layout::tree_layout(design);
        assert_eq!(geometry.list_height, crate::layout::TREE_VIEWPORT);
        assert!(geometry.height <= design.bar_height() + 640.0);
    }
    // Five options at the 96px pitch fit inside the tree window.
    const { assert!(4.0 * crate::tree::PITCH + 64.0 <= crate::layout::TREE_VIEWPORT) };
    assert_eq!(crate::tree::stage_width(680.0, 1920.0), 1440.0);
    assert_eq!(crate::tree::stage_width(680.0, 1280.0), 1216.0);
    assert_eq!(crate::tree::stage_width(680.0, 600.0), 680.0);
}
