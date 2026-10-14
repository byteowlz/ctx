//! Fixed synthetic poses for the hidden native tree proof, not provider inference.
use ctx_bar::model::{Suggestions, TreeVariant, ViewMode};
use ctx_bar::state::Status;
use ctx_bar::tree::Motion;
use gpui_kit::{Context, Window};

use super::Bar;

#[allow(dead_code)] // Only the separately compiled hidden-native tree harness calls these.
impl Bar {
    pub fn review_tree_scene(
        &mut self,
        variant: TreeVariant,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = if scene == "list-catalog" {
            ViewMode::List
        } else {
            ViewMode::Tree
        };
        self.config.view = view;
        self.config.tree_variant = variant;
        self.state.switch_view(view);
        self.state.edit("adjust appearance".into());
        self.local_choices = None;
        self.motion = Motion::default();
        self.catalog_ready = true; // Declared fixtures only; no discovery or network.
        self.complete_review();
        if matches!(scene, "child" | "preview" | "back" | "child-loading") {
            self.state.choose(0);
            if scene != "child-loading" {
                self.complete_review();
            }
        }
        match scene {
            "preview" | "back" => {
                self.state.choose(0);
                if scene == "back" {
                    self.state.back();
                }
            }
            "catalog" | "list-catalog" => {
                self.browse_all(cx);
            }
            "loading" => {
                self.state.edit("loading fixture".into());
            }
            "error" => {
                self.state.edit("error fixture".into());
                self.state.complete(
                    self.state.generation,
                    Err("Synthetic adapter unavailable".into()),
                );
            }
            "none" => {
                self.state.edit("no match fixture".into());
                self.state.complete(
                    self.state.generation,
                    Ok(Suggestions {
                        items: vec![],
                        source: "Jev via EAVS".into(),
                        latency_ms: 0.0,
                    }),
                );
            }
            "unicode" => {
                self.state.query = "亮度".into();
            }
            "idle" => {
                self.state.escape();
            }
            _ => {}
        }
        let query = self.state.query.clone();
        self.input.update(cx, |input, cx| {
            input.set_value(query, window, cx);
            input.focus(window, cx);
        });
        self.scroll.scroll_to_item(0);
        cx.notify();
    }
    pub fn review_tree_complete(&mut self, cx: &mut Context<Self>) {
        if self.state.status == Status::Loading {
            self.complete_review();
        }
        cx.notify();
    }
}
