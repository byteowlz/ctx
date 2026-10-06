use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};

use ctx_bar::config::{Config, descriptor_path};
use ctx_bar::model::{Catalog, Item, fixture_catalog};
use ctx_bar::proxy::Proxy;
use ctx_bar::state::{Selection, State, Status};

use crate::theme;

pub struct Bar {
    config: Config,
    input: Entity<InputState>,
    state: State,
    catalog: Catalog,
    catalog_ready: bool,
    connection: String,
    busy: bool,
    remaining: u64,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}
impl Bar {
    pub fn new(config: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("What do you want to do?"));
        let changed = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            match event {
                InputEvent::Change => {
                    this.state.edit(input.read(cx).value().to_string());
                    this.scroll.scroll_to_item(0);
                    this.schedule(true, cx);
                }
                InputEvent::PressEnter { .. } => this.choose(this.state.selected, cx),
                InputEvent::Focus => this.state.focus(window.is_window_active()),
                InputEvent::Blur => this.state.focus(false),
            }
            cx.notify();
        });
        let activated = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.state.focus(false);
                cx.notify()
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut bar = Self {
            state: State::new(config.platform, config.fixture, config.auto_select_first),
            config,
            input,
            catalog: fixture_catalog().unwrap_or(Catalog {
                items: vec![],
                branches: vec![],
            }),
            catalog_ready: false,
            connection: "Connecting to local Jev proxy…".into(),
            busy: false,
            remaining: 0,
            scroll: ScrollHandle::new(),
            _subscriptions: vec![changed, activated],
        };
        bar.load_catalog(cx);
        bar
    }

    fn load_catalog(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.catalog_ready = false;
        self.connection = "Connecting to local Jev proxy…".into();
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
                        this.connection =
                            "Local proxy catalog ready · synthetic fixture only".into();
                        this.schedule(true, cx)
                    }
                    Err(error) => {
                        this.connection = error.clone();
                        if this.state.status == Status::Loading {
                            this.state.complete(this.state.generation, Err(error));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// One in-flight network request; edits coalesce to the latest generation.
    /// No full capture or ctx CLI calls, synchronously or otherwise.
    fn schedule(&mut self, debounce: bool, cx: &mut Context<Self>) {
        if self.state.status != Status::Loading {
            return;
        }
        if !self.catalog_ready {
            if !self.busy {
                self.state
                    .complete(self.state.generation, Err(self.connection.clone()));
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
            self.state.complete(self.state.generation, Err("Branches unavailable: update the local proxy catalog, then retry. Flat remains available.".into()));
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
                    if this.busy
                        || generation != this.state.generation
                        || this.state.status != Status::Loading
                    {
                        return None;
                    }
                    this.busy = true;
                    Some((
                        this.state.request(),
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
            Selection::Preview(_receipt) => {} // Intentionally no invocation, launch, shell or posting API.
        }
        cx.notify();
    }
    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(selection) = self.state.choose(index) {
            self.selection(selection, cx)
        }
    }
    fn reset_options(&mut self, cx: &mut Context<Self>) {
        self.state.options(
            self.state.platform,
            self.state.fixture,
            self.state.presentation,
        );
        self.scroll.scroll_to_item(0);
        self.schedule(true, cx);
        cx.notify();
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
                self.state
                    .navigate(if event.keystroke.key == "up" { -1 } else { 1 });
                self.scroll.scroll_to_item(self.state.selected);
                window.prevent_default();
                cx.stop_propagation();
            }
            "escape" => {
                if self.state.node != "root" {
                    self.back(cx)
                } else {
                    self.state.escape();
                    self.input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                }
                window.prevent_default();
                cx.stop_propagation();
            }
            _ => return,
        }
        cx.notify();
    }
    fn overview(&self) -> Vec<Item> {
        [
            "oqto.open",
            "screenshot.capture",
            "tasks.add",
            "dictation.start",
            "selection.speak",
            "audio.transcribe",
            "system.settings",
        ]
        .iter()
        .filter_map(|id| {
            self.catalog
                .items
                .iter()
                .find(|i| &i.id == id && i.offered(self.state.platform, self.state.fixture))
                .cloned()
        })
        .take(5)
        .collect()
    }
    fn status_text(&self) -> String {
        match &self.state.status {
            Status::Overview => {
                "Catalog examples — type a request for real Jev suggestions.".into()
            }
            Status::Loading => "Asking Jev… older results cannot replace this request.".into(),
            Status::Ready => format!(
                "{} · {:.0} ms · choose an interface",
                self.state.source, self.state.latency_ms
            ),
            Status::NoMatch => {
                "No matching interface. Rephrase or choose another synthetic fixture.".into()
            }
            Status::Error(error) | Status::Preview(error) => error.clone(),
        }
    }
}

impl Render for Bar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let overview = self.state.status == Status::Overview;
        let rows = if overview {
            self.overview()
        } else {
            self.state.items.clone()
        };
        let selected = self.state.selected;
        let muted = theme.muted_foreground;
        let error = matches!(self.state.status, Status::Error(_));
        let state_text = self.status_text();
        let mut list = div()
            .id("suggestions")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col()
            .gap_1();
        for (index, item) in rows.into_iter().enumerate() {
            let label = item.label.clone();
            let branch = item.is_branch();
            let active = !overview && index == selected;
            let score = item
                .probability
                .map(|p| format!("{:.0}%", p * 100.0))
                .unwrap_or_default();
            list = list.child(
                div()
                    .id(SharedString::from(format!("suggestion-{index}")))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .min_h(px(62.))
                    .flex_shrink_0()
                    .rounded(px(theme.radius_lg.as_f32()))
                    .bg(if active {
                        theme.primary.alpha(0.13)
                    } else {
                        theme.popover.alpha(0.20)
                    })
                    .hover(|style| style.bg(theme.primary.alpha(0.10)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if overview {
                            this.input.update(cx, |input, cx| {
                                input.set_value(label.clone(), window, cx);
                                input.focus(window, cx)
                            });
                        } else {
                            this.choose(index, cx)
                        }
                    }))
                    .child(
                        Icon::new(icon(&item.icon))
                            .size(px(20.))
                            .text_color(if active { theme.primary } else { muted }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(theme.primary)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(item.tool),
                                    )
                                    .child(div().text_color(muted).child("/"))
                                    .child(div().text_color(muted).child(item.interface)),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_ellipsis()
                                    .child(item.label),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .text_ellipsis()
                                    .child(item.description),
                            ),
                    )
                    .child(div().text_size(px(11.)).text_color(muted).child(score))
                    .child(
                        Icon::new(if branch {
                            IconName::ChevronRight
                        } else {
                            IconName::CornerDownLeft
                        })
                        .size(px(16.))
                        .text_color(muted),
                    ),
            );
        }
        let mut panel = div()
            .id("ctx-bar")
            .key_context("CtxBar")
            .w_full()
            .h_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .text_color(theme.foreground)
            .text_size(px(13.5))
            .capture_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("ctx"),
                    )
                    .child(div().text_color(muted).child("Preview only"))
                    .child(div().flex_1())
                    .child(
                        Button::new("scheme")
                            .ghost()
                            .icon(if self.config.theme == theme::DARK {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            })
                            .label("Appearance")
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.cancel_timer();
                                this.config.theme = if this.config.theme == theme::DARK {
                                    theme::LIGHT
                                } else {
                                    theme::DARK
                                }
                                .into();
                                theme::apply(cx, &this.config.theme);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .bg(theme.popover.alpha(0.45))
                    .rounded(px(theme.radius_lg.as_f32()))
                    .px_2()
                    .child(
                        Input::new(&self.input)
                            .aria_label("Omnibar request; type or use system dictation")
                            .prefix(Icon::new(IconName::Search).text_color(theme.primary))
                            .h(px(52.))
                            .bordered(false)
                            .focus_bordered(true),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("platform")
                            .outline()
                            .label(format!("Platform: {}", self.state.platform.label()))
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.platform = this.state.platform.next();
                                this.reset_options(cx)
                            })),
                    )
                    .child(
                        Button::new("fixture")
                            .outline()
                            .label(format!("Fixture: {}", self.state.fixture.label()))
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.fixture = this.state.fixture.next();
                                this.reset_options(cx)
                            })),
                    )
                    .child(
                        Button::new("presentation")
                            .outline()
                            .label(self.state.presentation.label())
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.presentation = this.state.presentation.next();
                                this.reset_options(cx)
                            })),
                    ),
            )
            .child(div().text_size(px(11.)).text_color(muted).child(
                "Type or use system dictation · fixture context only, never desktop capture",
            ));
        if let Some(breadcrumb) = &self.state.breadcrumb {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("back")
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .label("Back")
                            .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                    )
                    .child(
                        div()
                            .text_color(muted)
                            .text_ellipsis()
                            .child(format!("Root / {breadcrumb}")),
                    ),
            );
        }
        panel = panel.child(
            div()
                .text_size(px(12.))
                .text_color(if error { theme.danger } else { muted })
                .child(state_text),
        );
        if matches!(self.state.status, Status::Loading) {
            panel = panel.child(
                div()
                    .flex_1()
                    .text_color(muted)
                    .child("Loading live suggestions…"),
            );
        } else {
            panel = panel.child(list)
        }
        panel
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("first-timeout")
                            .outline()
                            .label(format!(
                                "First after {}s: {}",
                                self.config.timeout_seconds,
                                if self.state.timeout_enabled {
                                    "on"
                                } else {
                                    "off"
                                }
                            ))
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.set_timeout(!this.state.timeout_enabled);
                                cx.notify()
                            })),
                    )
                    .child(div().flex_1())
                    .child(div().text_size(px(11.)).text_color(muted).child(
                        if self.state.timer.is_some() {
                            format!("First selection in {}s", self.remaining)
                        } else {
                            "↑ ↓ choose · Enter preview · Esc cancel".into()
                        },
                    ))
                    .child(
                        Button::new("retry")
                            .ghost()
                            .icon(IconName::RefreshCw)
                            .label("Retry")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.edit(this.state.query.clone());
                                this.load_catalog(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(muted)
                    .text_ellipsis()
                    .child(self.connection.clone()),
            )
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
