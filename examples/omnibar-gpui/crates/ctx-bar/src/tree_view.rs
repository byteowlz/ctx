//! Native adapter for the Studio moving-tree anatomy; shares the owning Bar state.
use ctx_bar::model::ViewMode;
use ctx_bar::state::Status;
use ctx_bar::tree::{Node, Plan, Position};
use gpui_kit::base::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{
    BoxShadow, Context, Div, InteractiveElement as _, ParentElement as _, PathBuilder,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point, px,
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
    /// Crumbs variant only: a compact centred path over the top edge of the
    /// tree window (Studio `#breadcrumbs`), never a band of its own.
    fn tree_breadcrumbs(&self, surface: &Frame, cx: &mut Context<Self>) -> Option<Div> {
        if self.config.tree_variant != ctx_bar::model::TreeVariant::Crumbs
            || (self.state.breadcrumb.is_none() && self.state.receipt.is_none())
        {
            return None;
        }
        let mut path = self.state.query.clone();
        if let Some(parent) = &self.state.breadcrumb {
            path.push_str(&format!(" / {parent}"));
        }
        if let Some(receipt) = &self.state.receipt {
            path.push_str(&format!(" / {}", receipt.item.label));
        }
        Some(
            div()
                .absolute()
                .top(px(4.0))
                .left_0()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .max_w(px(self.config.width))
                        .min_w_0()
                        .child(surface.text(path, Role::Navigation, false, cx)),
                ),
        )
    }
    /// The tree region below the bar: one dark stage with feathered edges
    /// (the mockup's black stage, without a window-like box), the moving tree,
    /// plain-text navigation and the status line underneath.
    pub(super) fn render_tree(
        &mut self,
        surface: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use ctx_bar::layout::{TREE_GAP, TREE_MARGIN, TREE_VIEWPORT};
        let plan = Plan::new(
            &self.state,
            self.config.tree_variant,
            self.window_width - 2.0 * STAGE_INSET,
            TREE_VIEWPORT,
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
        let ground = surface.ink(false).background;
        let stage = div()
            .absolute()
            .top_0()
            .bottom(px(TREE_MARGIN))
            .left(px(STAGE_INSET))
            .right(px(STAGE_INSET))
            .rounded(px(28.0))
            .bg(ground.opacity(0.86))
            .shadow(vec![BoxShadow {
                color: ground.opacity(0.5),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(STAGE_INSET - 4.0),
                spread_radius: px(0.0),
                inset: false,
            }]);
        div()
            .relative()
            .w_full()
            .mt(px(TREE_GAP))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .pb(px(TREE_MARGIN))
            .child(stage)
            .child(
                div()
                    .mx(px(STAGE_INSET))
                    .child(self.tree_panel(&plan, offset, surface, cx)),
            )
            .child(self.tree_controls(surface, window, cx))
            .child(self.tree_status(surface, cx))
    }
    fn tree_status(&self, surface: &Frame, cx: &mut Context<Self>) -> Div {
        // The preview node already names the choice; the status line only says
        // what happens next (the list view keeps its full sentence).
        let message = match (&self.state.status, &self.execution) {
            (Status::Preview(_), None) if self.preview_action().is_some() => {
                "Preview only. Enter again or Run to run it on this Mac.".to_string()
            }
            (Status::Preview(_), None) => "Preview only. Nothing executed.".to_string(),
            _ => self.status_text(),
        };
        let mut line = div()
            .h(px(ctx_bar::layout::TREE_STATUS))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center();
        if !message.is_empty() {
            line = line.child(
                div()
                    .max_w(px(self.window_width - 2.0 * STAGE_INSET - 32.0))
                    .min_w_0()
                    .child(surface.text(
                        message,
                        super::status_role(&self.state.status),
                        false,
                        cx,
                    )),
            );
        }
        line
    }
    pub(super) fn tree_panel(
        &self,
        plan: &Plan,
        offset: Position,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        use ctx_bar::tree::{APERTURE, APERTURE_PREVIEW};
        let height = plan.center.y * 2.0;
        let ink = surface.ink(false);
        let width = plan.node_width;
        let mut panel = div().id("moving-tree").relative().w_full().h(px(height)).flex_shrink_0()
            .overflow_hidden().track_focus(&self.tree_focus).tab_index(0)
            .role(gpui_kit::accesskit::Role::ListBox).aria_label("Preview choices")
            .aria_description("Up/Down select, Enter previews, F4 Back. Only an explicit Run executes a whitelisted action.");
        panel = panel.child(tree_edges(plan, offset, ink.foreground));
        if let Some(position) = plan.query {
            let p = position.plus(offset);
            panel = panel.child(
                div()
                    .absolute()
                    .left(px(p.x - width / 2.0))
                    .top(px(p.y - 32.0))
                    .w(px(width))
                    .h(px(64.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .opacity(0.42)
                    .child(div().min_w_0().child(surface.text(
                        self.state.query.clone(),
                        Role::Label,
                        false,
                        cx,
                    ))),
            );
        }
        for node in &plan.nodes {
            panel = panel.child(self.tree_node(plan, node, offset, surface, cx));
        }
        // The pinned aperture (not the rejected idle/input focus mark).
        let aperture = if matches!(self.state.status, Status::Preview(_)) {
            APERTURE_PREVIEW
        } else {
            APERTURE
        };
        panel = panel.child(
            div()
                .absolute()
                .left(px(plan.center.x - (width + 16.0) / 2.0))
                .top(px(plan.center.y - aperture / 2.0))
                .w(px(width + 16.0))
                .h(px(aperture))
                .border_t_1()
                .border_b_1()
                .border_color(ink.foreground),
        );
        if let Some(crumbs) = self.tree_breadcrumbs(surface, cx) {
            panel = panel.child(crumbs);
        }
        panel
    }
    /// Studio node: text only (the stage is the ground), the label wrapping to
    /// two lines at the node width, metadata beneath; only the selected or
    /// hovered node gets the selection fill.
    fn tree_node(
        &self,
        plan: &Plan,
        node: &Node,
        offset: Position,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let width = plan.node_width;
        let preview = matches!(self.state.status, Status::Preview(_)) && !node.prior;
        let lines = wrap_label(&node.item.label, surface.columns(width - 12.0, Role::Label));
        let meta = surface.line_height(Role::Navigation);
        let height =
            (8.0 + lines.len() as f32 * surface.line_height(Role::Label) + 2.0 + meta).max(64.0);
        let p = node.position.plus(offset);
        let selection = surface.ink(true).selection;
        let mut row = div()
            .id(SharedString::from(format!(
                "tree-node-{}-{}-{}",
                node.position.x, node.prior, node.item.id
            )))
            .absolute()
            .left(px(p.x - width / 2.0))
            .top(px(p.y - height / 2.0))
            .w(px(width))
            .h(px(height))
            .px(px(6.0))
            .py(px(4.0))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(2.0));
        if node.selected && !node.prior {
            row = row.bg(selection);
        }
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
                .hover(move |style| style.bg(selection))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tree_click(generation, index, &id, cx);
                }));
        }
        for line in lines {
            row = row.child(surface.text(line, Role::Label, node.selected, cx));
        }
        let likely = node.index == Some(0) && !self.state.provisional && !node.prior;
        let detail = if preview {
            "Preview only".to_string()
        } else if likely {
            format!("{} / likely", node.item.tool)
        } else {
            node.item.tool.clone()
        };
        row = row.child(div().opacity(0.75).child(surface.text(
            detail,
            Role::Navigation,
            node.selected,
            cx,
        )));
        row
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
    /// Plain-text navigation as in the mockup; disabled actions dim to 0.4.
    pub(super) fn tree_controls(
        &self,
        surface: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let ready = self.state.status == Status::Ready || self.state.provisional;
        let can_choose = ready && self.tree_can_choose();
        let can_run = self.preview_action().is_some() && !self.executing;
        let can_back = self.state.history_len() > 0;
        let can_pause = self.state.timer.is_some();
        let can_resume = ready
            && !self.state.provisional
            && self.state.timeout_enabled
            && self.tree_can_choose();
        let _ = window;
        let control =
            |id: &'static str, label: &'static str, enabled: bool, cx: &mut Context<Self>| {
                Button::new(id)
                    .ghost()
                    .accessibility_label(label)
                    .disabled(!enabled)
                    .opacity(if enabled { 1.0 } else { 0.4 })
                    .child(surface.text(label, Role::Navigation, false, cx))
            };
        let mut row = div()
            .h(px(ctx_bar::layout::TREE_NAVIGATION))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(24.0))
            .child(
                control("tree-back", "Back", can_back, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
            )
            .child(control("tree-choose", "Choose", can_choose, cx).on_click(
                cx.listener(|this, _, window, cx| this.choose(this.state.selected, window, cx)),
            ))
            .child(
                control("tree-run", "Run", can_run, cx).on_click(cx.listener(|this, _, _, cx| {
                    this.execute_preview(cx);
                })),
            )
            .child(
                control("tree-pause", "Pause", can_pause, cx).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.state.pause_dwell();
                        cx.notify();
                    },
                )),
            )
            .child(
                control("tree-resume", "Resume", can_resume, cx).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.state.focus(window.is_window_active());
                        if this.state.resume_dwell() {
                            this.arm_timer(cx);
                        }
                        cx.notify();
                    },
                )),
            );
        if let Some(details) = self.preview_details_button(surface, Role::Navigation, cx) {
            row = row.child(details);
        }
        row
    }
    pub(super) fn preview_details_button(
        &self,
        surface: &Frame,
        role: Role,
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
                .child(surface.text("Details", role, false, cx)),
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
        if let Some(details) = self.preview_details_button(surface, Role::Control, cx) {
            panel = panel.child(details);
        }
        surface.finish(panel)
    }
}

/// Studio edge: from the parent's right edge, horizontal to the midpoint,
/// vertical to the child's row, horizontal into the child. Chosen paths at
/// 0.65, others at 0.24, one hairline each; edges share the node layer offset.
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
                    window.paint_path(path, ink.opacity(if *chosen { 0.65 } else { 0.24 }));
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Space kept between the stage and the window edge for its feathered shadow.
pub(super) const STAGE_INSET: f32 = 24.0;

/// Greedy word wrap into at most two lines; overflow stays on the second line,
/// where the text element ellipsizes it.
fn wrap_label(text: &str, columns: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![String::new()];
    for word in text.split_whitespace() {
        let full = lines.len() >= 2;
        let current = lines.last_mut().expect("one line");
        if current.is_empty() {
            current.push_str(word);
        } else if !full && current.chars().count() + 1 + word.chars().count() > columns {
            lines.push(word.to_string());
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    lines
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap_label;

    #[test]
    fn wraps_at_word_boundaries_into_at_most_two_lines() {
        assert_eq!(
            wrap_label("Notification controls", 18),
            ["Notification", "controls"]
        );
        assert_eq!(wrap_label("Sound controls", 18), ["Sound controls"]);
        assert_eq!(
            wrap_label("Choose an Omarchy theme for today please", 12),
            ["Choose an", "Omarchy theme for today please"]
        );
        assert_eq!(wrap_label("", 10), [""]);
    }
}
