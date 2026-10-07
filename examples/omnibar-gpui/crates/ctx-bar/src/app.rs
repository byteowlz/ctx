use std::ops::Range;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, WindowBackgroundAppearance, div, px, size,
};

use ctx_bar::config::{Config, descriptor_path};
use ctx_bar::layout::{INSET, layout};
use ctx_bar::model::{Catalog, Item, fixture_catalog};
use ctx_bar::proxy::Proxy;
use ctx_bar::state::{Selection, State, Status};
use ctx_bar_design::{BarDesign, theme_choices};

use crate::presentation;

#[path = "surfaces.rs"]
mod surfaces;
use surfaces::{Frame, Role};

#[cfg(any(feature = "native-review", test))]
#[path = "surface_review.rs"]
pub mod surface_review;

pub struct Bar {
    config: Config,
    input: Entity<InputState>,
    state: State,
    local_themes: Option<Vec<BarDesign>>,
    catalog: Catalog,
    catalog_ready: bool,
    connection: String,
    busy: bool,
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
        state.presentation = config.presentation;
        let height = layout(config.design, 0, false, config.height).height;
        let mut bar = Self {
            state,
            config,
            input,
            local_themes: None,
            catalog: fixture_catalog().unwrap_or(Catalog {
                items: vec![],
                branches: vec![],
            }),
            catalog_ready: false,
            connection: "Start the local omnibar adapter, then edit to retry.".into(),
            busy: false,
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
                    this.state.edit(input.read(cx).value().to_string());
                    this.local_themes = theme_choices(&this.state.query);
                    this.scroll.scroll_to_item(0);
                    #[cfg(feature = "native-review")]
                    if this.review {
                        this.complete_review();
                    }
                    this.schedule(true, cx);
                }
                InputEvent::PressEnter { .. } => this.choose(this.state.selected, window, cx),
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
                this.state.cancel_timer();
            }
            #[cfg(feature = "native-review")]
            presentation::diagnose(cx, |d| d.observer_repaints += 1);
            cx.notify(); // Repaint native caret/selection pixel ink.
        });
        let activated = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.state.focus(false);
                cx.notify()
            }
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

    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        let Some(ticket) = self.state.timer.clone() else {
            return;
        };
        let seconds = self.config.timeout_seconds;
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
        if let Some(choices) = &self.local_themes {
            let Some(design) = choices.get(index).copied() else {
                return;
            };
            self.config.design = design;
            window.set_background_appearance(if design == BarDesign::Lens {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            });
            self.state.escape();
            self.local_themes = None;
            self.input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            self.scroll.scroll_to_item(0);
            cx.notify();
        } else if let Some(selection) = self.state.choose(index) {
            self.selection(selection, cx)
        }
    }
    fn back(&mut self, cx: &mut Context<Self>) {
        if self.state.back() {
            self.scroll.scroll_to_item(0);
            self.schedule(false, cx);
            cx.notify()
        }
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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
            "escape" => {
                self.escape_input(window, cx);
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    fn navigate(&mut self, delta: isize) -> bool {
        let count = match &self.local_themes {
            Some(choices) => choices.len(),
            None if self.state.status == Status::Ready => self.state.items.len(),
            _ => return false,
        };
        if count == 0 {
            return false;
        }
        if self.local_themes.is_some() {
            self.state.cancel_timer();
            self.state.selected =
                (self.state.selected as isize + delta).rem_euclid(count as isize) as usize;
        } else {
            self.state.navigate(delta);
        }
        self.scroll.scroll_to_item(self.state.selected);
        true
    }

    fn escape_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.node != "root" {
            self.back(cx);
            return;
        }
        self.state.escape();
        self.local_themes = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn status_text(&self) -> String {
        if let Some(choices) = &self.local_themes {
            return if choices.is_empty() {
                "No design matches. Type ctx theme to see all ten.".into()
            } else {
                String::new()
            };
        }
        if self.state.timer.is_some() {
            return format!("First preview in {}s · nothing executes", self.remaining);
        }
        match &self.state.status {
            Status::Overview | Status::Ready => String::new(),
            Status::Loading => "Asking Jev…".into(),
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
        self.local_themes = theme_choices(query);
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
        surface_review::populate(&mut self.state, &self.catalog, scene);
        cx.notify();
    }

    /// Explicit synthetic review seam for first-party input dispatch. Ordinary
    /// runs never expose a handle through this hook; no transport or capture.
    #[cfg(feature = "native-review")]
    #[allow(dead_code)] // Consumed by the separately compiled hidden-native example.
    pub fn review_input(&self) -> Option<Entity<InputState>> {
        self.review.then(|| self.input.clone())
    }

    #[cfg(feature = "native-review")]
    pub fn review_snapshot(&self, cx: &gpui_kit::App) -> serde_json::Value {
        serde_json::json!({
            "design": self.config.design.id(), "query": self.state.query,
            "input_empty": self.state.query.is_empty() && self.input.read(cx).value().is_empty(),
            "local_choices": self.local_themes.as_ref().map(Vec::len),
            "selected": self.state.selected, "node": self.state.node,
            "routable": self.state.remote_request().is_some(),
            "timer": self.state.timer.is_some(), "generation": self.state.generation,
            "input_diagnostics": cx.try_global::<presentation::InputDiagnostics>(),
            "surface_diagnostics": cx.try_global::<surfaces::Diagnostics>(),
        })
    }

    #[cfg(feature = "native-review")]
    fn complete_review(&mut self) {
        if !self.state.query.is_empty() && self.local_themes.is_none() {
            let items = self
                .catalog
                .items
                .iter()
                .filter(|item| {
                    ["appearance.light", "system.settings", "appearance.dark"]
                        .contains(&item.id.as_str())
                })
                .cloned()
                .collect();
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
        let count = self.local_themes.as_ref().map_or_else(
            || {
                if self.state.status == Status::Ready {
                    self.state.items.len()
                } else {
                    0
                }
            },
            Vec::len,
        );
        let breadcrumb_height = if self.state.breadcrumb.is_some() {
            ctx_bar::layout::STATUS_HEIGHT
        } else {
            0.0
        };
        let mut geometry = layout(
            self.config.design,
            count,
            !message.is_empty(),
            self.config.height - breadcrumb_height,
        );
        geometry.height += breadcrumb_height;
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
        if let Some(breadcrumb) = &self.state.breadcrumb {
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
        if count > 0 {
            panel = panel.child(self.result_list(geometry.list_height, &surface, cx));
        }
        panel
    }
}

impl Bar {
    fn result_list(
        &self,
        height: f32,
        surface: &Frame,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ink = surface.ink(false);
        let mut list = div()
            .id("suggestions")
            .mt(px(INSET))
            .h(px(height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col();
        if let Some(choices) = &self.local_themes {
            for (index, design) in choices.iter().copied().enumerate() {
                list = list.child(
                    surface
                        .row(index == self.state.selected)
                        .id(SharedString::from(format!("design-{}", design.id())))
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
                                    design.label(),
                                    Role::Label,
                                    index == self.state.selected,
                                    cx,
                                ))
                                .child(surface.text(
                                    design.description(),
                                    Role::Metadata,
                                    index == self.state.selected,
                                    cx,
                                )),
                        )
                        .child(surface.text(
                            "Local design",
                            Role::ToolMetadata,
                            index == self.state.selected,
                            cx,
                        )),
                );
            }
        } else {
            for (index, item) in self.state.items.iter().enumerate() {
                list = list.child(
                    surface
                        .row(index == self.state.selected)
                        .id(SharedString::from(format!("suggestion-{index}")))
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.choose(index, window, cx)),
                        )
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
                                .child(surface.text(
                                    item.label.clone(),
                                    Role::Label,
                                    index == self.state.selected,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .text_size(px(11.))
                                        .child(surface.text(
                                            item.tool.clone(),
                                            Role::ToolMetadata,
                                            index == self.state.selected,
                                            cx,
                                        ))
                                        .child(div().flex_1().min_w_0().child(surface.text(
                                            item.interface.clone(),
                                            Role::Metadata,
                                            index == self.state.selected,
                                            cx,
                                        ))),
                                ),
                        )
                        .child(
                            Icon::new(if item.is_branch() {
                                IconName::ChevronRight
                            } else {
                                IconName::CornerDownLeft
                            })
                            .size(px(16.))
                            .text_color(ink.metadata),
                        ),
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
