use std::ops::Range;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    AppContext as _, Context, Entity, EntityInputHandler as _, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    WindowBackgroundAppearance, div, px, size,
};

use ctx_bar::commands::{Action, choices};
use ctx_bar::config::{Config, descriptor_path};
use ctx_bar::layout::{INSET, layout};
use ctx_bar::model::{Catalog, Item, Presentation, ViewMode, fixture_catalog};
use ctx_bar::proxy::Proxy;
use ctx_bar::state::{Selection, State, Status, TimerTicket};
use ctx_bar::tree::Motion;
use ctx_bar_design::BarDesign;

use crate::presentation;

#[path = "local_retrieval.rs"]
mod local_retrieval;

#[path = "tree_view.rs"]
mod tree_view;

#[path = "surfaces.rs"]
mod surfaces;
use surfaces::{Frame, Role};

#[cfg(any(feature = "native-review", test))]
#[path = "surface_review.rs"]
pub mod surface_review;

#[cfg(feature = "native-review")]
#[path = "tree_review.rs"]
mod tree_review;

pub struct Bar {
    config: Config,
    input: Entity<InputState>,
    state: State,
    local_choices: Option<Vec<Action>>,
    tree_focus: FocusHandle,
    motion: Motion,
    armed_timer: Option<TimerTicket>,
    catalog: Catalog,
    catalog_ready: bool,
    connection: String,
    busy: bool,
    local_busy: bool,
    local_generation: Option<u64>,
    remaining: u64,
    height: f32,
    review: bool,
    last_selection: Range<usize>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}
impl Bar {
    pub fn new(config: Config, review: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type..."));
        let subscriptions = Self::subscribe_input(&input, window, cx);
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut state = State::new(
            config.platform,
            config.fixture,
            config.auto_select_first && !review,
        );
        state.presentation = if config.view == ViewMode::Tree {
            Presentation::Branches
        } else {
            config.presentation
        };
        state.view = config.view;
        let height = layout(config.design, 0, false, config.height).height;
        let mut bar = Self {
            state,
            config,
            input,
            local_choices: None,
            tree_focus: cx.focus_handle(),
            motion: Motion::default(),
            armed_timer: None,
            catalog: fixture_catalog().unwrap_or(Catalog {
                items: vec![],
                branches: vec![],
                retrieval_enabled: false,
            }),
            catalog_ready: false,
            connection: "Start the local omnibar adapter, then edit to retry.".into(),
            busy: false,
            local_busy: false,
            local_generation: None,
            remaining: 0,
            height,
            review,
            last_selection: 0..0,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        // Review scenes never read a descriptor or contact a decision service.
        if !review {
            bar.load_catalog(cx);
        }
        bar
    }

    fn subscribe_input(
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let changed = cx.subscribe_in(input, window, |this, input, event, window, cx| {
            match event {
                InputEvent::Change => {
                    #[cfg(feature = "native-review")]
                    presentation::diagnose(cx, |d| d.changes += 1);
                    if !this.state.input_change(input.read(cx).value().to_string()) {
                        return;
                    }
                    this.local_choices = choices(&this.state.query);
                    this.motion = Motion::default();
                    this.scroll.scroll_to_item(0);
                    #[cfg(feature = "native-review")]
                    if this.review {
                        this.complete_review();
                    }
                    this.schedule(true, cx);
                }
                InputEvent::PressEnter { .. } => {
                    if !this.composing(window, cx) {
                        this.choose(this.state.selected, window, cx);
                    }
                }
                InputEvent::Focus => this.state.focus(window.is_window_active()),
                InputEvent::Blur => this.state.focus(false),
            }
            cx.notify();
        });
        let selection = cx.observe(input, |this, input, cx| {
            #[cfg(feature = "native-review")]
            presentation::diagnose(cx, |d| d.observations += 1);
            let range = input.read(cx).selected_range();
            if range != this.last_selection {
                this.last_selection = range;
                this.state.pause_dwell();
            }
            #[cfg(feature = "native-review")]
            presentation::diagnose(cx, |d| d.observer_repaints += 1);
            cx.notify(); // Repaint native caret/selection pixel ink.
        });
        let activated = cx.observe_window_activation(window, |this, window, cx| {
            this.state.focus(window.is_window_active());
            cx.notify()
        });
        vec![changed, selection, activated]
    }

    fn load_catalog(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.catalog_ready = false;
        let path = descriptor_path();
        let timeout = self.config.request_timeout_seconds;
        let task = cx.background_executor().spawn(async move {
            path.and_then(|path| Proxy::new(&path, timeout)?.catalog())
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(catalog) => {
                        this.catalog = catalog;
                        this.catalog_ready = true;
                        this.schedule(true, cx)
                    }
                    Err(error) => {
                        this.connection =
                            format!("{error}. Start the local adapter, then edit to retry.");
                        if this.state.remote_request().is_some() {
                            this.state
                                .complete(this.state.generation, Err(this.connection.clone()));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Local presentation commands never enter the transport path. Edits coalesce
    /// to one latest-generation network request; no desktop capture is performed.
    fn schedule(&mut self, debounce: bool, cx: &mut Context<Self>) {
        if self.review || self.state.remote_request().is_none() {
            return;
        }
        if !self.catalog_ready {
            if !self.busy {
                self.load_catalog(cx);
            }
            return;
        }
        if self.state.presentation == ctx_bar::model::Presentation::Branches
            && !self
                .catalog
                .items
                .iter()
                .chain(&self.catalog.branches)
                .any(Item::is_branch)
        {
            self.state.complete(
                self.state.generation,
                Err(
                    "Branches unavailable; use flat presentation or update the local adapter."
                        .into(),
                ),
            );
            cx.notify();
            return;
        }
        self.schedule_local(cx);
        let generation = self.state.generation;
        let delay = if debounce { self.config.debounce_ms } else { 0 };
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let work = this
                .update(cx, |this, _cx| {
                    if this.busy || generation != this.state.generation {
                        return None;
                    }
                    let request = this.state.remote_request()?;
                    this.busy = true;
                    Some((
                        request,
                        this.catalog.clone(),
                        this.config.request_timeout_seconds,
                    ))
                })
                .ok()
                .flatten();
            let Some((request, catalog, timeout)) = work else {
                return;
            };
            let path = descriptor_path();
            let task = cx.background_executor().spawn(async move {
                path.and_then(|path| Proxy::new(&path, timeout)?.suggest(&request, &catalog))
                    .map_err(|error| error.to_string())
            });
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.state.complete(generation, result) {
                    this.scroll.scroll_to_item(0);
                    this.arm_timer(cx);
                } else if generation != this.state.generation {
                    this.schedule(true, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn timer_work(&mut self) -> Option<(TimerTicket, u64)> {
        let ticket = self.state.timer.clone()?;
        if self.armed_timer.as_ref() == Some(&ticket) {
            return None;
        }
        self.armed_timer = Some(ticket.clone());
        let seconds = if self.state.view == ViewMode::Tree {
            2
        } else {
            self.config.timeout_seconds
        };
        Some((ticket, seconds))
    }
    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        let Some((ticket, seconds)) = self.timer_work() else {
            return;
        };
        self.remaining = seconds;
        cx.spawn(async move |this, cx| {
            for remaining in (0..seconds).rev() {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep = this
                    .update(cx, |this, cx| {
                        if this.state.timer.as_ref() != Some(&ticket) {
                            return false;
                        }
                        this.remaining = remaining;
                        if remaining == 0
                            && let Some(selection) = this.state.timer_fire(&ticket)
                        {
                            this.selection(selection, cx)
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    return;
                }
            }
        })
        .detach();
    }
    fn selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        match selection {
            Selection::Expand(_) => {
                self.scroll.scroll_to_item(0);
                self.schedule(false, cx)
            }
            Selection::Preview(_) => {} // Never an invocation, shell, launch or capture.
        }
        cx.notify();
    }
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(action) = self
            .local_choices
            .as_ref()
            .and_then(|choices| choices.get(index))
            .copied()
        {
            self.apply_local(action, window, cx);
        } else if self.tree_can_choose()
            && let Some(selection) = self.state.choose(index)
        {
            self.selection(selection, cx)
        }
    }
    fn apply_local(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Design(design) => {
                self.config.design = design;
                window.set_background_appearance(if design == BarDesign::Lens {
                    WindowBackgroundAppearance::Blurred
                } else {
                    WindowBackgroundAppearance::Transparent
                });
            }
            Action::View(view) => {
                self.config.view = view;
                self.state.switch_view(view);
            }
            Action::Variant(variant) => {
                self.config.tree_variant = variant;
                self.config.view = ViewMode::Tree;
                self.state.switch_view(ViewMode::Tree);
            }
            Action::Dwell(enabled) => {
                self.config.auto_select_first = enabled;
                self.state.set_timeout(enabled);
            }
        }
        self.state.escape();
        self.local_choices = None;
        self.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.scroll.scroll_to_item(0);
        cx.notify();
    }
    fn tree_can_choose(&self) -> bool {
        self.state.view != ViewMode::Tree || (self.state.tree_is_settled() && !self.tree_moving())
    }
    fn tree_moving(&self) -> bool {
        self.state.view == ViewMode::Tree && !self.motion.settled(Instant::now())
    }
    fn toggle_view(&mut self, cx: &mut Context<Self>) {
        let view = self.state.view.next();
        self.state.switch_view(view);
        self.config.view = view;
        self.local_choices = choices(&self.state.query);
        self.scroll.scroll_to_item(0);
        self.schedule(true, cx);
    }
    fn choose_candidate(
        &mut self,
        generation: u64,
        index: usize,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.tree_can_choose()
            && let Some(selection) = self.state.choose_current(generation, index, id)
        {
            self.selection(selection, cx);
        }
    }
    fn back(&mut self, cx: &mut Context<Self>) {
        if self.state.back() {
            self.scroll.scroll_to_item(0);
            self.schedule(false, cx);
            cx.notify()
        }
    }
    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        })
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(window, cx) {
            self.state.pause_dwell();
            return;
        }
        if event.keystroke.modifiers.shift
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                let delta = if event.keystroke.key == "up" { -1 } else { 1 };
                if !self.navigate(delta) {
                    return;
                }
            }
            "escape" => self.escape_key(window, cx),
            "f2" => self.browse_all(cx),
            "f3" => self.toggle_view(cx),
            "f4" => self.back(cx),
            key => {
                if !self.tree_key(key, window, cx) {
                    return;
                }
            }
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    fn escape_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree_active() {
            self.state.pause_dwell();
            self.input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.escape_input(window, cx);
        }
    }
    fn tree_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.state.view != ViewMode::Tree || !self.tree_focus.is_focused(window) {
            return false;
        }
        match key {
            "left" => self.back(cx),
            "right" | "enter" => self.choose(self.state.selected, window, cx),
            "space" => {
                self.state.focus(window.is_window_active());
                if self.state.resume_dwell() {
                    self.arm_timer(cx);
                }
            }
            _ => return false,
        }
        true
    }
    fn navigate(&mut self, delta: isize) -> bool {
        let count = match &self.local_choices {
            Some(choices) => choices.len(),
            None if self.state.status == Status::Ready || self.state.provisional => {
                self.state.items.len()
            }
            _ => return false,
        };
        if count == 0 {
            return false;
        }
        if self.local_choices.is_some() {
            self.state.pause_dwell();
            self.state.selected =
                (self.state.selected as isize + delta).rem_euclid(count as isize) as usize;
        } else if self.state.view == ViewMode::Tree {
            let next = ctx_bar::tree::neighbor(count, self.state.selected, delta);
            self.state
                .navigate(next as isize - self.state.selected as isize);
        } else {
            self.state.navigate(delta);
        }
        self.scroll.scroll_to_item(self.state.selected);
        true
    }

    fn escape_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.history_len() > 0 || self.state.node != "root" {
            self.back(cx);
            return;
        }
        self.state.escape();
        self.local_choices = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn status_text(&self) -> String {
        if let Some(choices) = &self.local_choices {
            return if choices.is_empty() {
                "No local choice matches. Use ctx theme, ctx view, ctx tree or ctx dwell.".into()
            } else {
                String::new()
            };
        }
        if self.state.timer.is_some() {
            return format!("First preview in {}s · nothing executes", self.remaining);
        }
        if let Some(note) = &self.state.local_note {
            return note.clone();
        }
        match &self.state.status {
            Status::Ready if self.review => "Synthetic preview · F2: all".into(),
            Status::Ready if self.state.view == ViewMode::Tree && self.state.timeout_enabled => {
                "Jev · F2: all · dwell paused · Resume".into()
            }
            Status::Ready if self.catalog_ready => "Jev · F2: all".into(),
            Status::Overview | Status::Ready => String::new(),
            Status::Loading => "Asking Jev…".into(),
            Status::NoMatch if self.catalog.retrieval_enabled => "No Jev match · F2: all".into(),
            Status::NoMatch => "No matching interface. Edit the request to retry.".into(),
            Status::Error(error) | Status::Preview(error) => error.clone(),
        }
    }

    /// Explicit synthetic review support, only invoked by --review-dir. Does not
    /// run decision transport or read desktop context. User text is never used.
    #[cfg(feature = "native-review")]
    pub fn review_scene(
        &mut self,
        design: BarDesign,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.review {
            return;
        }
        self.config.design = design;
        self.input
            .update(cx, |input, cx| input.set_value(query, window, cx));
        self.state.edit(query.into());
        self.local_choices = choices(query);
        self.complete_review();
        cx.notify();
    }

    /// Extra hidden synthetic states for the full-surface trial. Never transport.
    #[cfg(feature = "native-review")]
    #[allow(dead_code)] // Local source-pinned harness, not a foreground command.
    pub fn review_surface_scene(
        &mut self,
        design: BarDesign,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.review {
            return;
        }
        let Some(scene) = surface_review::Scene::ALL
            .into_iter()
            .find(|candidate| candidate.id() == scene)
        else {
            return;
        };
        self.review_scene(design, scene.query(), window, cx);
        self.catalog.retrieval_enabled = matches!(
            scene,
            surface_review::Scene::Retrieval
                | surface_review::Scene::Catalog
                | surface_review::Scene::RetrievalUnavailable
                | surface_review::Scene::RetrievalRefusal
        );
        surface_review::populate(&mut self.state, &self.catalog, scene);
        cx.notify();
    }

    /// Feature-only input seam for hidden synthetic review or explicit transport proofs.
    /// Obtaining the handle does not enable fixture routing; normal builds omit it.
    #[cfg(feature = "native-review")]
    #[allow(dead_code)] // Consumed by the separately compiled hidden-native example.
    pub fn review_input(&self) -> Option<Entity<InputState>> {
        Some(self.input.clone())
    }

    #[cfg(feature = "native-review")]
    pub fn review_snapshot(&self, cx: &gpui_kit::App) -> serde_json::Value {
        let tree = self.tree_active().then(|| {
            let top = self.config.design.bar_height() + 64.0;
            let viewport = self.height - top - 32.0;
            let plan = ctx_bar::tree::Plan::new(&self.state, self.config.tree_variant, self.config.width, viewport);
            let selected = plan.selected_center().map(|position| [position.x, position.y + top]);
            serde_json::json!({"viewport_top":top,"viewport_height":viewport,"center":[plan.center.x,plan.center.y + top],"selected_center":selected,"node_width":plan.node_width,"current_width":plan.current_width,"depth":plan.depth,"visible_current":plan.nodes.iter().filter(|node| !node.prior).count(),"prior_nodes":plan.nodes.iter().filter(|node| node.prior).count(),"edges":plan.edges.len()})
        });
        serde_json::json!({
            "tree": tree,
            "design": self.config.design.id(), "query": self.state.query,
            "input_empty": self.state.query.is_empty() && self.input.read(cx).value().is_empty(),
            "local_choices": self.local_choices.as_ref().map(Vec::len),
            "view": self.state.view, "tree_variant": self.config.tree_variant,
            "tree_moving": self.tree_moving(), "tree_settled": self.state.tree_is_settled(),
            "catalog_count": self.catalog.candidates(&self.state.request()).map_or(0, |items| items.len()),
            "chosen_label": self.state.items.get(self.state.selected).map(|item| &item.label),
            "selected": self.state.selected, "node": self.state.node, "presentation": self.state.presentation,
            "receipt": self.state.receipt.as_ref().map(|receipt| serde_json::json!({"item":receipt.item,"request":receipt.request,"source":receipt.source})),
            "routable": self.state.remote_request().is_some(),
            "timer": self.state.timer.is_some(), "generation": self.state.generation,
            "status": format!("{:?}", self.state.status), "source": self.state.source,
            "provisional": self.state.provisional, "ids": self.state.items.iter().map(|item| &item.id).collect::<Vec<_>>(),
            "retrieval_enabled": self.catalog.retrieval_enabled,
            "receipt_source": self.state.receipt.as_ref().map(|receipt| &receipt.source),
            "input_diagnostics": cx.try_global::<presentation::InputDiagnostics>(),
            "surface_diagnostics": cx.try_global::<surfaces::Diagnostics>(),
        })
    }

    #[cfg(feature = "native-review")]
    fn complete_review(&mut self) {
        if !self.state.query.is_empty() && self.local_choices.is_none() {
            let items = if self.state.presentation == Presentation::Branches {
                self.catalog
                    .candidates(&self.state.request())
                    .unwrap_or_default()
                    .into_iter()
                    .take(5)
                    .cloned()
                    .collect()
            } else {
                self.catalog
                    .items
                    .iter()
                    .filter(|item| {
                        ["appearance.light", "system.settings", "appearance.dark"]
                            .contains(&item.id.as_str())
                    })
                    .cloned()
                    .collect()
            };
            self.state.complete(
                self.state.generation,
                Ok(ctx_bar::model::Suggestions {
                    items,
                    source: "synthetic-native-review-not-Jev".into(),
                    latency_ms: 0.0,
                }),
            );
        }
    }
}

impl Render for Bar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "native-review")]
        {
            presentation::diagnose(cx, |d| d.renders += 1);
            surfaces::begin_frame(cx);
        }
        let theme = cx.theme().clone();
        let surface = Frame::new(self.config.design, theme.color_tokens(), !theme.is_dark());
        let message = self.status_text();
        let count = self.local_choices.as_ref().map_or_else(
            || {
                if self.state.status == Status::Ready || self.state.provisional {
                    self.state.items.len()
                } else {
                    0
                }
            },
            Vec::len,
        );
        let tree_active = self.tree_active();
        let preview_controls = !tree_active && self.state.receipt.is_some();
        let geometry = self.frame_layout(count, !message.is_empty());
        if (self.height - geometry.height).abs() > 0.5 {
            self.height = geometry.height;
            window.resize(size(px(self.config.width), px(self.height)));
        }
        let input = presentation::input(self.config.design, &self.input, window, cx);
        let mut panel = div()
            .id("ctx-bar")
            .key_context("CtxBar")
            .w_full()
            .h_full()
            .p(px(INSET))
            .flex()
            .flex_col()
            .text_color(theme.foreground)
            .text_size(px(13.5))
            .capture_key_down(cx.listener(Self::key_down))
            .child(input);
        if tree_active {
            panel = panel.child(self.tree_path_header(&surface, cx));
        } else if let Some(breadcrumb) = &self.state.breadcrumb {
            panel = panel.child(
                surface.finish(
                    surface
                        .panel()
                        .h(px(ctx_bar::layout::STATUS_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("back")
                                .ghost()
                                .icon(IconName::ChevronLeft)
                                .accessibility_label("Back")
                                .text_color(surface.ink(false).foreground)
                                .child(surface.back_label(window, cx))
                                .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                        )
                        .child(div().flex_1().min_w_0().child(surface.text(
                            breadcrumb.clone(),
                            Role::Navigation,
                            false,
                            cx,
                        ))),
                ),
            );
        }
        if !message.is_empty() {
            panel = panel.child(
                surface.finish(
                    surface
                        .panel()
                        .h(px(ctx_bar::layout::STATUS_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px_3()
                        .child(div().w_full().min_w_0().child(surface.text(
                            message,
                            status_role(&self.state.status),
                            false,
                            cx,
                        ))),
                ),
            );
        }
        if tree_active {
            panel = panel.child(self.render_tree(geometry.list_height, &surface, window, cx));
        } else if count > 0 {
            panel = panel.child(self.result_list(geometry.list_height, &surface, cx));
        }
        if preview_controls {
            panel = panel.child(self.list_preview_controls(&surface, window, cx));
        }
        panel
    }
}

impl Bar {
    fn frame_layout(&self, count: usize, status: bool) -> ctx_bar::layout::Layout {
        if self.tree_active() {
            return ctx_bar::layout::tree_layout(self.config.design, self.config.height);
        }
        let breadcrumb =
            u8::from(self.state.breadcrumb.is_some()) as f32 * ctx_bar::layout::STATUS_HEIGHT;
        let preview =
            u8::from(self.state.receipt.is_some()) as f32 * ctx_bar::layout::STATUS_HEIGHT;
        let mut geometry = layout(
            self.config.design,
            count,
            status,
            self.config.height - breadcrumb - preview,
        );
        geometry.height += breadcrumb + preview;
        geometry
    }
    fn candidate_row(
        &self,
        index: usize,
        item: &ctx_bar::model::Item,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let ink = surface.ink(false);
        let selected = index == self.state.selected;
        let generation = self.state.generation;
        let id = item.id.clone();
        let metadata = div()
            .flex()
            .gap_2()
            .text_size(px(11.))
            .child(surface.text(item.tool.clone(), Role::ToolMetadata, selected, cx))
            .child(div().flex_1().min_w_0().child(surface.text(
                item.interface.clone(),
                Role::Metadata,
                selected,
                cx,
            )));
        let trailing = if item.is_branch() {
            IconName::ChevronRight
        } else {
            IconName::CornerDownLeft
        };
        surface
            .row(selected)
            .id(SharedString::from(format!("suggestion-{index}")))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.choose_candidate(generation, index, &id, cx);
            }))
            .child(
                Icon::new(icon(&item.icon))
                    .size(px(18.))
                    .text_color(ink.metadata),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(surface.text(item.label.clone(), Role::Label, selected, cx))
                    .child(metadata),
            )
            .child(Icon::new(trailing).size(px(16.)).text_color(ink.metadata))
    }
    fn result_list(
        &self,
        height: f32,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut list = div()
            .id("suggestions")
            .mt(px(INSET))
            .h(px(height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col();
        if let Some(choices) = &self.local_choices {
            for (index, action) in choices.iter().copied().enumerate() {
                list = list.child(
                    surface
                        .row(index == self.state.selected)
                        .id(SharedString::from(format!("local-control-{index}")))
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.choose(index, window, cx)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .min_w_0()
                                .child(surface.text(
                                    action.label(),
                                    Role::Label,
                                    index == self.state.selected,
                                    cx,
                                ))
                                .child(surface.text(
                                    action.detail(),
                                    Role::Metadata,
                                    index == self.state.selected,
                                    cx,
                                )),
                        )
                        .child(surface.text(
                            "Local control",
                            Role::ToolMetadata,
                            index == self.state.selected,
                            cx,
                        )),
                );
            }
        } else {
            let offset = f32::from(self.scroll.offset().y);
            for (index, item) in self.state.items.iter().enumerate() {
                list = list.child(
                    if ctx_bar::layout::live_row(index, self.state.selected, offset, height) {
                        self.candidate_row(index, item, surface, cx)
                            .into_any_element()
                    } else {
                        div()
                            .h(px(ctx_bar::layout::ROW_HEIGHT))
                            .flex_shrink_0()
                            .into_any_element()
                    },
                );
            }
        }
        // Enclosure stays in the viewport, outside the unchanged native scroller.
        surface.finish(surface.panel().h(px(height)).flex_shrink_0().child(list))
    }
}

fn status_role(status: &Status) -> Role {
    match status {
        Status::Error(_) => Role::Error,
        Status::Preview(_) => Role::Preview,
        _ => Role::Status,
    }
}

fn icon(name: &str) -> IconName {
    match name {
        "sun" => IconName::Sun,
        "moon" => IconName::Moon,
        "sliders" => IconName::Settings,
        "palette" => IconName::Palette,
        "volume" => IconName::Volume2,
        "mic" => IconName::Mic,
        "display" => IconName::Monitor,
        "bell" => IconName::Bell,
        "wifi" => IconName::Wifi,
        "bluetooth" => IconName::Bluetooth,
        "shield" => IconName::Shield,
        "lock" => IconName::Lock,
        "power" => IconName::Power,
        "folder" => IconName::Folder,
        "clipboard" => IconName::Clipboard,
        "capture" => IconName::Camera,
        "workspace" => IconName::PanelsTopLeft,
        "check" => IconName::Check,
        "history" => IconName::Clock,
        _ => IconName::Search,
    }
}
