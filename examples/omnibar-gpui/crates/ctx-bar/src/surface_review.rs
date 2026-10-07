//! Bounded, synthetic whole-surface fixtures; compiled only for review/tests.
//! Exercises the incumbent State/flat list/one declared branch, not a new tree.
use ctx_bar::model::{Catalog, Presentation, Suggestions};
use ctx_bar::state::State;

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Scene {
    Empty,
    Local,
    Tools,
    Branch,
    Child,
    Loading,
    Error,
    NoMatch,
    Preview,
    Long,
    Unicode,
}

impl Scene {
    pub const ALL: [Self; 11] = [
        Self::Empty,
        Self::Local,
        Self::Tools,
        Self::Branch,
        Self::Child,
        Self::Loading,
        Self::Error,
        Self::NoMatch,
        Self::Preview,
        Self::Long,
        Self::Unicode,
    ];

    pub fn id(self) -> &'static str {
        // Closed fixture metadata, in discriminant order; no product-role logic.
        const IDS: [&str; Scene::ALL.len()] = [
            "empty", "local", "tools", "branch", "child", "loading", "error", "no-match",
            "preview", "long", "unicode",
        ];
        IDS[self as usize]
    }

    pub fn query(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Local => "ctx theme",
            _ => "light mode",
        }
    }
}

pub(super) fn populate(state: &mut State, catalog: &Catalog, scene: Scene) {
    state.presentation = Presentation::Flat;
    state.edit(scene.query().into());
    if matches!(scene, Scene::Empty | Scene::Local | Scene::Loading) {
        return;
    }
    if matches!(scene, Scene::Error) {
        state.complete(
            state.generation,
            Err("Synthetic adapter unavailable. Edit to retry.".into()),
        );
        return;
    }
    let items = if matches!(scene, Scene::Branch | Scene::Child) {
        state.presentation = Presentation::Branches;
        catalog.branches.iter().take(3).cloned().collect()
    } else if matches!(scene, Scene::NoMatch) {
        vec![]
    } else {
        catalog.items.iter().take(3).cloned().collect()
    };
    state.complete(
        state.generation,
        Ok(Suggestions {
            items,
            source: "synthetic-native-surface-review-not-Jev".into(),
            latency_ms: 0.0,
        }),
    );
    match scene {
        Scene::Child => {
            state.choose(0);
            let items = catalog
                .branches
                .iter()
                .find(|b| b.node.as_deref() == Some(&state.node))
                .map(|b| {
                    catalog
                        .items
                        .iter()
                        .filter(|i| b.children.as_ref().is_some_and(|ids| ids.contains(&i.id)))
                        .take(3)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            state.complete(
                state.generation,
                Ok(Suggestions {
                    items,
                    source: "synthetic-native-surface-review-not-Jev".into(),
                    latency_ms: 0.0,
                }),
            );
        }
        Scene::Preview => {
            state.choose(0);
        }
        Scene::Long => {
            if let Some(item) = state.items.first_mut() {
                item.label = "Synthetic long interface label with retained native semantics and bounded clipping".into();
                item.interface = "synthetic.long.interface.with.extra.metadata".into();
            }
        }
        Scene::Unicode => {
            if let Some(item) = state.items.first_mut() {
                item.label = "Café 日本語 — native fallback".into();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Role, status_role};
    use ctx_bar::model::{Fixture, Platform, fixture_catalog};
    use ctx_bar::state::Status;

    #[test]
    fn fixture_discriminants_and_ids_cover_the_closed_scene_table() {
        let expected = [
            "empty", "local", "tools", "branch", "child", "loading", "error", "no-match",
            "preview", "long", "unicode",
        ];
        for (index, scene) in Scene::ALL.into_iter().enumerate() {
            assert_eq!(scene as usize, index);
            assert_eq!(scene.id(), expected[index]);
        }
    }

    #[test]
    fn theme_tool_branch_status_and_preview_fixtures_stay_native_and_preview_only() {
        let catalog = fixture_catalog().unwrap();
        for scene in Scene::ALL {
            assert!(!scene.id().is_empty());
            let mut state = State::new(Platform::Macos, Fixture::Desktop, false);
            populate(&mut state, &catalog, scene);
            assert_eq!(state.query, scene.query());
            assert!(state.timer.is_none());
            match scene {
                Scene::Local => {
                    assert_eq!(
                        ctx_bar_design::theme_choices(&state.query).unwrap().len(),
                        10
                    );
                    assert!(state.remote_request().is_none());
                }
                Scene::Tools | Scene::Branch | Scene::Long | Scene::Unicode => {
                    assert_eq!(state.status, Status::Ready);
                    assert_eq!(state.items.len(), 3);
                    if matches!(scene, Scene::Branch) {
                        assert!(state.items.iter().all(|i| i.is_branch()));
                    }
                }
                Scene::Child => {
                    assert_ne!(state.node, "root");
                    assert!(state.breadcrumb.is_some());
                    assert!(!state.items.is_empty());
                    assert!(state.back());
                    assert_eq!(state.node, "root");
                }
                Scene::Preview => {
                    assert!(state.receipt.is_some());
                    assert_eq!(status_role(&state.status), Role::Preview);
                    assert!(state.remote_request().is_none());
                    assert!(state.choose(0).is_none());
                }
                Scene::Error => assert_eq!(status_role(&state.status), Role::Error),
                Scene::Loading => assert_eq!(status_role(&state.status), Role::Status),
                Scene::NoMatch => assert_eq!(state.status, Status::NoMatch),
                Scene::Empty => assert_eq!(state.status, Status::Overview),
            }
        }
    }
}
