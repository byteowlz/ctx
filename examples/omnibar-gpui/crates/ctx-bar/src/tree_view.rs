//! Native adapter for the Studio moving-tree anatomy; shares the owning Bar state.
use ctx_bar::model::ViewMode;
use ctx_bar::state::Status;
use ctx_bar::tree::{Node, Plan, Position};
use gpui_kit::base::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, PathBuilder, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point, px,
};

use std::time::Instant;

use super::Bar;
use super::surfaces::{Frame, Role};

impl Bar {
    pub(super) fn tree_active(&self) -> bool {
        self.state.view == ViewMode::Tree
            && self.local_choices.is_none()
            && (!self.state.items.is_empty()
                || self.state.breadcrumb.is_some()
                || matches!(self.state.status, Status::Preview(_)))
    }
    pub(super) fn tree_path_header(&self, surface: &Frame, cx: &mut Context<Self>) -> Div {
        let text = if self.config.tree_variant == ctx_bar::model::TreeVariant::Crumbs {
            let mut path = self.state.query.clone();
            if let Some(parent) = &self.state.breadcrumb {
                path.push_str(&format!(" / {parent}"));
            }
            if let Some(receipt) = &self.state.receipt {
                path.push_str(&format!(" / {}", receipt.item.label));
            }
            path
        } else {
            self.state
                .breadcrumb
                .clone()
                .unwrap_or_else(|| "Root choices".into())
        };
        surface.finish(
            surface
                .panel()
                .h(px(32.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_3()
                .child(div().w_full().min_w_0().child(surface.text(
                    text,
                    Role::Navigation,
                    false,
                    cx,
                ))),
        )
    }
    pub(super) fn render_tree(
        &mut self,
        height: f32,
        surface: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let plan = Plan::new(
            &self.state,
            self.config.tree_variant,
            self.config.width,
            height,
        );
        let now = Instant::now();
        self.motion.retarget(
            plan.target,
            now,
            self.config.reduce_motion || cx.reduce_motion(),
        );
        let offset = self.motion.position(now);
        if self.motion.settled(now) {
            self.state.settle_tree(self.state.generation);
            self.arm_timer(cx);
        } else {
            window.request_animation_frame();
        }
        div()
            .w_full()
            .h(px(height + 32.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(self.tree_panel(&plan, offset, surface, cx))
            .child(self.tree_controls(surface, window, cx))
    }
    pub(super) fn tree_panel(
        &self,
        plan: &Plan,
        offset: Position,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let height = plan.center.y * 2.0;
        let ink = surface.ink(false);
        let mut panel = surface.panel().id("moving-tree").relative().h(px(height)).flex_shrink_0()
            .overflow_hidden().track_focus(&self.tree_focus).tab_index(0)
            .role(gpui_kit::accesskit::Role::ListBox).aria_label("Preview choices")
            .aria_description("Synthetic control declarations. Up/Down select, Enter previews, F4 Back. No system action is executed.");
        let width = plan.node_width;
        panel = panel.child(tree_edges(plan, offset, ink.foreground));
        if let Some(position) = plan.query {
            let p = position.plus(offset);
            panel = panel.child(
                surface
                    .row(false)
                    .px(px(6.0))
                    .absolute()
                    .left(px(p.x - width / 2.0))
                    .top(px(p.y - 28.0))
                    .w(px(width))
                    .opacity(0.42)
                    .child(div().min_w_0().flex_1().child(surface.text(
                        self.state.query.clone(),
                        Role::Label,
                        false,
                        cx,
                    ))),
            );
        }
        let preview = matches!(self.state.status, Status::Preview(_));
        for node in &plan.nodes {
            panel = panel.child(self.tree_node(plan, node, offset, surface, cx));
        }
        // This is the pinned tree aperture, not the rejected idle/input focus mark.
        let aperture_height = if preview {
            124.0_f32.min(height - 4.0)
        } else {
            64.0
        };
        panel = panel.child(
            div()
                .absolute()
                .left(px(plan.center.x - width / 2.0 - 8.0))
                .top(px(plan.center.y - aperture_height / 2.0))
                .w(px(plan.current_width + 16.0))
                .h(px(aperture_height))
                .border_t_1()
                .border_b_1()
                .border_color(ink.foreground),
        );
        surface.finish(panel)
    }
    fn tree_node(
        &self,
        plan: &Plan,
        node: &Node,
        offset: Position,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let left = plan.node_width / 2.0;
        let width = plan.width_of(node);
        let current_preview = matches!(self.state.status, Status::Preview(_)) && !node.prior;
        let p = node.position.plus(offset);
        let row_height = if current_preview {
            112.0_f32.min(plan.center.y * 2.0 - 8.0)
        } else {
            56.0
        };
        let mut row = surface
            .row(node.selected)
            .px(px(6.0))
            .id(SharedString::from(format!(
                "tree-node-{}-{}-{}",
                node.position.x, node.prior, node.item.id
            )))
            .absolute()
            .left(px(p.x - left))
            .top(px(p.y - row_height / 2.0))
            .w(px(width))
            .h(px(row_height));
        if node.prior {
            row = row.opacity(if node.selected { 0.62 } else { 0.42 });
        }
        if let Some(index) = node.index {
            let generation = self.state.generation;
            let id = node.item.id.clone();
            row = row
                .role(gpui_kit::accesskit::Role::ListBoxOption)
                .aria_label(node.item.label.clone())
                .aria_description(node.item.description.clone())
                .aria_selected(node.selected);
            if node.selected {
                row = row.aria_active_descendant();
            }
            row = row
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tree_click(generation, index, &id, cx);
                }));
        }
        let mut content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(surface.text(node.item.label.clone(), Role::Label, node.selected, cx))
            .child(surface.text(
                format!("{} / {}", node.item.tool, node.item.interface),
                Role::Metadata,
                node.selected,
                cx,
            ));
        if current_preview {
            content = content.child(surface.text("Nothing executed", Role::Metadata, true, cx));
        }
        row.child(content)
    }
    fn tree_click(&mut self, generation: u64, index: usize, id: &str, cx: &mut Context<Self>) {
        if generation != self.state.generation
            || self.state.items.get(index).is_none_or(|item| item.id != id)
        {
            return;
        }
        if self.state.selected == index && self.tree_can_choose() {
            self.choose_candidate(generation, index, id, cx);
        } else {
            self.state
                .navigate(index as isize - self.state.selected as isize);
            cx.notify();
        }
    }
    pub(super) fn tree_controls(
        &self,
        surface: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let ready = self.state.status == Status::Ready || self.state.provisional;
        let can_choose = ready && self.tree_can_choose();
        let can_back = self.state.history_len() > 0;
        let can_pause = self.state.timer.is_some();
        let can_resume = ready
            && !self.state.provisional
            && self.state.timeout_enabled
            && self.tree_can_choose();
        let mut panel = surface
            .panel()
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Button::new("tree-back")
                    .ghost()
                    .accessibility_label("Back")
                    .disabled(!can_back)
                    .opacity(if can_back { 1.0 } else { 0.42 })
                    .child(surface.back_label(window, cx))
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
            )
            .child(
                Button::new("tree-choose")
                    .ghost()
                    .accessibility_label("Choose preview")
                    .disabled(!can_choose)
                    .opacity(if can_choose { 1.0 } else { 0.42 })
                    .child(surface.text("Choose", Role::Control, false, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.choose(this.state.selected, window, cx)
                    })),
            )
            .child(
                Button::new("tree-pause")
                    .ghost()
                    .accessibility_label("Pause preview dwell")
                    .disabled(!can_pause)
                    .opacity(if can_pause { 1.0 } else { 0.42 })
                    .child(surface.text("Pause", Role::Control, false, cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.pause_dwell();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("tree-resume")
                    .ghost()
                    .accessibility_label("Resume preview dwell")
                    .disabled(!can_resume)
                    .opacity(if can_resume { 1.0 } else { 0.42 })
                    .child(surface.text("Resume", Role::Control, false, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.state.focus(window.is_window_active());
                        if this.state.resume_dwell() {
                            this.arm_timer(cx);
                        }
                        cx.notify();
                    })),
            );
        if let Some(details) = self.preview_details_button(surface, cx) {
            panel = panel.child(details);
        }
        surface.finish(panel)
    }
    pub(super) fn preview_details_button(
        &self,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> Option<Button> {
        let receipt = self.state.receipt.as_ref()?;
        let details = format!(
            "{}\n\n{}\n\nUnverified platform mechanism: {}\n\nSynthetic preview only. No command ran; no permission was granted; host support was not checked.",
            receipt.item.label,
            receipt.item.description,
            receipt
                .item
                .support_for(self.state.platform)
                .unwrap_or("Requires separate capability and permission discovery.")
        );
        Some(
            Button::new("preview-details")
                .ghost()
                .tooltip(details.clone())
                .accessibility_label(format!("Preview details: {details}"))
                .child(surface.text("Details", Role::Control, false, cx)),
        )
    }
    pub(super) fn list_preview_controls(
        &self,
        surface: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut panel = surface
            .panel()
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Button::new("preview-back")
                    .ghost()
                    .accessibility_label("Back")
                    .child(surface.back_label(window, cx))
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
            );
        if let Some(details) = self.preview_details_button(surface, cx) {
            panel = panel.child(details);
        }
        surface.finish(panel)
    }
}

/// Orthogonal connectors share the node layer's offset, so they move together.
fn tree_edges(plan: &Plan, offset: Position, ink: gpui_kit::Hsla) -> gpui_kit::Canvas<()> {
    let width = plan.node_width;
    let edges: Vec<_> = plan
        .edges
        .iter()
        .map(|edge| (edge.from.plus(offset), edge.to.plus(offset), edge.chosen))
        .collect();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for (from, to, chosen) in &edges {
                let start = bounds.origin + point(px(from.x + width / 2.0), px(from.y));
                let end = bounds.origin + point(px(to.x - width / 2.0), px(to.y));
                let middle = (start.x + end.x) / 2.0;
                let mut path = PathBuilder::stroke(px(1.0));
                path.move_to(start);
                path.line_to(point(middle, start.y));
                path.line_to(point(middle, end.y));
                path.line_to(end);
                if let Ok(path) = path.build() {
                    window.paint_path(path, ink.opacity(if *chosen { 0.5 } else { 0.24 }));
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
