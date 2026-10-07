//! Full-surface *trial*, Studio full-surface-tree-r1 at 6428621; not approval.
//! Static Dot text uses the same actual 11px Departure raster as input: 2x for
//! labels/navigation/status, 1.5x metadata, 1x Back's native-size label.
//! Static gap .4 (rather than input's frozen .25) avoids solid glyphs at 2x;
//! corner .34. Only lit cells are submitted; there is no full-window field.
//! Native labels remain in layout/accessibility, controls/icons remain native.
//! Unsupported text, exhausted budgets or font failure retain visible native ink.
//! No editing, selection, transport, execution or input-mask logic lives here.
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use ctx_bar_design::BarDesign;
use gpui_kit::base::ColorTokens;
use gpui_kit::{
    AnyElement, App, Bounds, BoxShadow, Div, FontWeight, Global, Hsla, InteractiveElement as _,
    IntoElement as _, ParentElement as _, PathBuilder, Pixels, Styled as _, Window, canvas, div,
    fill, point, px, rgb, size,
};

use crate::matrix::{CORNER, FONT_BYTES, FONT_FAMILY, Glyph, Rect, atlas};

const STATIC_GAP: f32 = 0.4;
const MAX_TEXT_BYTES: usize = 256;
const MAX_TEXT_CELLS: usize = 4096;
const MAX_FRAME_CELLS: usize = 24_000;
const CACHE_ENTRIES: usize = 64;

#[cfg(feature = "native-review")]
#[derive(Default, serde::Serialize)]
pub(super) struct Diagnostics {
    labels: usize,
    pixel_labels: usize,
    native_fallbacks: usize,
    reserved_cells: usize,
    visible_cells: usize,
    label_bounds: Vec<[f32; 5]>,
    enclosure_bounds: Vec<[f32; 4]>,
}
#[cfg(feature = "native-review")]
impl Global for Diagnostics {}

#[cfg(feature = "native-review")]
pub(super) fn begin_frame(cx: &mut App) {
    if cx
        .try_global::<crate::presentation::InputDiagnostics>()
        .is_some()
    {
        cx.set_global(Diagnostics::default());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    Label,
    Metadata,
    Control,
    ToolMetadata,
    Navigation,
    Status,
    Error,
    Preview,
}

impl Role {
    fn scale(self) -> f32 {
        if self == Self::Control {
            return 1.0;
        }
        if matches!(self, Self::Metadata | Self::ToolMetadata) {
            return 1.5;
        }
        2.0
    }
}

#[derive(Clone, Copy)]
pub(super) struct Ink {
    pub background: Hsla,
    pub selection: Hsla,
    pub foreground: Hsla,
    pub metadata: Hsla,
    pub error: Hsla,
}

/// Canonical role pairings, including the existing light accent correction.
fn ink(design: BarDesign, colors: &ColorTokens, light: bool, _selected: bool) -> Ink {
    let mut paper = colors.background;
    paper.a = 1.0;
    let mut ink = Ink {
        background: colors.background,
        selection: colors.secondary,
        foreground: colors.foreground,
        metadata: if light {
            colors.foreground
        } else {
            colors.muted_foreground
        },
        // Destructive ink fails normal-text contrast on these panel grounds.
        // Use canonical foreground and an explicit Error: copy marker instead.
        error: colors.foreground,
    };
    match design {
        BarDesign::DotMatrix => {
            ink.background = rgb(0x000000).into();
            ink.selection = rgb(0x252525).into();
            ink.foreground = rgb(0xffffff).into();
            ink.metadata = rgb(0xcccccc).into();
            ink.error = ink.foreground; // Error copy remains explicit; no gold/red stage.
        }
        BarDesign::Monolith => {
            ink.background = colors.foreground;
            ink.selection = colors.foreground;
            ink.foreground = paper;
            ink.metadata = paper;
            ink.error = paper;
        }
        BarDesign::Signal => {
            ink.background = colors.primary;
            ink.selection = colors.primary;
            ink.foreground = if light {
                paper
            } else {
                colors.primary_foreground
            };
            ink.metadata = ink.foreground;
            ink.error = ink.foreground;
        }
        BarDesign::Lens | BarDesign::Notch => ink.background = colors.surface,
        _ => {}
    }
    ink
}

#[derive(Clone)]
pub(super) struct Frame {
    design: BarDesign,
    colors: ColorTokens,
    light: bool,
    spent: Rc<Cell<usize>>,
}

impl Frame {
    pub fn new(design: BarDesign, colors: ColorTokens, light: bool) -> Self {
        Self {
            design,
            colors,
            light,
            spent: Rc::new(Cell::new(0)),
        }
    }

    pub fn ink(&self, selected: bool) -> Ink {
        ink(self.design, &self.colors, self.light, selected)
    }

    /// One enclosure per disclosed group, never a replacement interaction tree.
    pub fn panel(&self) -> Div {
        let ink = self.ink(false);
        let design = self.design;
        let colors = self.colors;
        let mut panel = div().relative().text_color(ink.foreground);
        if design != BarDesign::Notch {
            panel = panel.bg(ink.background);
        }
        if design == BarDesign::Lens {
            panel = panel.rounded(px(14.0)).bg(ink.background.opacity(0.94));
        }
        if design != BarDesign::Notch {
            return panel;
        }
        panel.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    enclosure(design, bounds, &colors, window);
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    /// Edge ink must follow row backgrounds; otherwise native scene ordering
    /// hides Corners/Slot/Underline. Notch's filled silhouette stays behind text.
    pub fn finish<T: gpui_kit::ParentElement>(&self, panel: T) -> T {
        let design = self.design;
        let colors = self.colors;
        if !matches!(
            design,
            BarDesign::Corners | BarDesign::Slot | BarDesign::Underline
        ) {
            return panel;
        }
        panel.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    #[cfg(feature = "native-review")]
                    if cx.try_global::<Diagnostics>().is_some() {
                        cx.global_mut::<Diagnostics>().enclosure_bounds.push([
                            bounds.origin.x.into(),
                            bounds.origin.y.into(),
                            bounds.size.width.into(),
                            bounds.size.height.into(),
                        ]);
                    }
                    #[cfg(not(feature = "native-review"))]
                    let _ = cx;
                    enclosure(design, bounds, &colors, window);
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    pub fn row(&self, selected: bool) -> Div {
        let ink = self.ink(selected);
        let background = if self.design == BarDesign::Notch {
            ink.background.opacity(0.0)
        } else {
            ink.selection
        };
        let mut row = div()
            .h(px(ctx_bar::layout::ROW_HEIGHT))
            .flex_shrink_0()
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .bg(if selected {
                background
            } else {
                ink.background.opacity(0.0)
            });
        let edged = self.design != BarDesign::DotMatrix;
        if edged {
            row = row.border_b_1().border_color(if selected {
                ink.foreground
            } else {
                ink.background.opacity(0.0)
            });
        }
        if self.design == BarDesign::Lens {
            row = row.rounded(px(14.0));
        }
        // GPUI accepts one hover refinement; a second call panics at rendering.
        row.hover(move |style| {
            let style = style.bg(background);
            if edged {
                style.border_color(ink.foreground)
            } else {
                style
            }
        })
    }

    /// Preserve the native Back label width (default Button's 1rem text),
    /// icon, padding and hitbox. Only label ink changes, not the control.
    pub fn back_label(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        use gpui_kit::component::ActiveTheme as _;
        let font = gpui_kit::font(cx.theme().font_family.clone());
        let width = window
            .text_system()
            .shape_line(
                "Back".into(),
                window.rem_size(),
                &[gpui_kit::TextRun {
                    len: 4,
                    font,
                    color: self.ink(false).foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width;
        let label = if self.design == BarDesign::DotMatrix {
            self.text("Back", Role::Control, false, cx)
        } else {
            // Native control metrics are an explicit platform exception. Using
            // the variant's 15px/bold/mono body style clipped this fixed hitbox.
            div()
                .font_family(cx.theme().font_family.clone())
                .text_size(window.rem_size())
                .font_weight(FontWeight::NORMAL)
                .text_color(self.ink(false).foreground)
                .child("Back")
                .into_any_element()
        };
        div().w(width).child(label).into_any_element()
    }

    pub fn text(
        &self,
        text: impl Into<String>,
        role: Role,
        selected: bool,
        cx: &mut App,
    ) -> AnyElement {
        let text = text.into();
        let text = if role == Role::Error {
            format!("Error: {text}")
        } else {
            text
        };
        let ink = self.ink(selected);
        let color = match role {
            Role::Metadata => ink.metadata,
            Role::ToolMetadata
                if !self.light
                    && !matches!(
                        self.design,
                        BarDesign::DotMatrix | BarDesign::Monolith | BarDesign::Signal
                    ) =>
            {
                self.colors.primary
            }
            Role::ToolMetadata => ink.metadata,
            Role::Error => ink.error,
            _ => ink.foreground,
        };
        let scale = role.scale();
        let plan = if self.design == BarDesign::DotMatrix {
            let plan = cached_plan(&text, cx);
            plan.filter(|plan| {
                reserve(
                    &self.spent,
                    plan.cells.len() + plan_for_ellipsis().map_or(0, |p| p.cells.len()),
                )
            })
        } else {
            None
        };
        #[cfg(feature = "native-review")]
        if cx.try_global::<Diagnostics>().is_some() {
            let d = cx.global_mut::<Diagnostics>();
            d.labels += 1;
            d.pixel_labels += usize::from(plan.is_some());
            d.native_fallbacks +=
                usize::from(self.design == BarDesign::DotMatrix && plan.is_none());
            d.reserved_cells = self.spent.get();
        }
        let text_size = if self.design == BarDesign::DotMatrix {
            11.0 * scale
        } else if matches!(role, Role::Metadata | Role::ToolMetadata) {
            11.0
        } else if self.design == BarDesign::Unframed {
            15.0
        } else {
            13.5
        };
        let line_height = if self.design == BarDesign::DotMatrix {
            14.0 * scale
        } else {
            text_size + 4.0
        };
        let mut label = div()
            .relative()
            .min_w_0()
            .h(px(line_height))
            .text_size(px(text_size))
            .line_height(px(line_height))
            .text_ellipsis()
            .text_color(color);
        if self.design == BarDesign::DotMatrix {
            label = label.font_family(FONT_FAMILY);
        }
        if self.design == BarDesign::Prompt {
            use gpui_kit::component::ActiveTheme as _;
            label = label.font_family(cx.theme().mono_font_family.clone());
        }
        if self.design != BarDesign::DotMatrix
            && (role == Role::ToolMetadata
                || (matches!(self.design, BarDesign::Monolith | BarDesign::Notch)
                    && role != Role::Metadata))
        {
            label = label.font_weight(FontWeight::SEMIBOLD);
        }
        // Static labels only: replace ink *after* a complete raster and reservation.
        // Input is never transparent/disabled. Failed plans use normal native text.
        if let Some(plan) = plan {
            label = label.text_color(color.opacity(0.0));
            label = label.child(text).child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let count = paint_text(&plan, scale, bounds, color, window);
                        #[cfg(feature = "native-review")]
                        if cx.try_global::<Diagnostics>().is_some() {
                            let d = cx.global_mut::<Diagnostics>();
                            d.visible_cells += count;
                            if role == Role::Label {
                                d.label_bounds.push([
                                    bounds.origin.x.into(),
                                    bounds.origin.y.into(),
                                    bounds.size.width.into(),
                                    bounds.size.height.into(),
                                    scale,
                                ]);
                            }
                        }
                        #[cfg(not(feature = "native-review"))]
                        let _ = (count, cx);
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        } else {
            label = label.child(text);
        }
        label.into_any_element()
    }
}

fn reserve(spent: &Cell<usize>, count: usize) -> bool {
    let total = spent.get().saturating_add(count);
    if total > MAX_FRAME_CELLS {
        return false;
    }
    spent.set(total);
    true
}

#[derive(Debug)]
struct Plan {
    cells: Vec<Rect>,
    advance: f32,
}

/// Complete text support, not a secret replacement alphabet or mixed fallback.
fn plan(text: &str) -> Option<Plan> {
    if text.len() > MAX_TEXT_BYTES || !text.chars().all(|ch| static_glyph(ch).is_some()) {
        return None;
    }
    let mut cells = Vec::new();
    let mut advance: f32 = 0.0;
    for ch in text.chars() {
        let glyph = static_glyph(ch)?;
        let metrics = glyph.metrics;
        for (index, coverage) in glyph.bitmap.iter().enumerate() {
            if *coverage < 128 {
                continue;
            }
            let x = advance.round() + metrics.xmin as f32 + (index % metrics.width) as f32;
            let y =
                11.0 - metrics.ymin as f32 - metrics.height as f32 + (index / metrics.width) as f32;
            cells.push(Rect {
                x: x + STATIC_GAP / 2.0,
                y: y + STATIC_GAP / 2.0,
                width: 1.0 - STATIC_GAP,
                height: 1.0 - STATIC_GAP,
            });
            if cells.len() > MAX_TEXT_CELLS {
                return None;
            }
        }
        advance += metrics.advance_width;
    }
    Some(Plan { cells, advance })
}

/// Static-only punctuation from the *actual* font for incumbent status copy.
/// The frozen input atlas/support/IME guard is deliberately not extended.
fn static_glyph(ch: char) -> Option<&'static Glyph> {
    if let Some(glyph) = atlas()?.glyph(ch) {
        return Some(glyph);
    }
    let index = match ch {
        '—' => 0,
        '·' => 1,
        _ => return None,
    };
    use std::sync::OnceLock;
    static PUNCTUATION: OnceLock<Option<Vec<Glyph>>> = OnceLock::new();
    PUNCTUATION
        .get_or_init(|| {
            let font =
                fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default()).ok()?;
            ['—', '·']
                .into_iter()
                .map(|ch| {
                    if !font.has_glyph(ch) {
                        return None;
                    }
                    let (metrics, bitmap) = font.rasterize(ch, 11.0);
                    if !bitmap.iter().any(|coverage| *coverage >= 128) {
                        return None;
                    }
                    Some(Glyph {
                        metrics,
                        bitmap,
                        index: font.lookup_glyph_index(ch),
                    })
                })
                .collect()
        })
        .as_ref()?
        .get(index)
}

#[derive(Default)]
struct Cache(VecDeque<(String, Arc<Plan>)>);
impl Global for Cache {}

fn cached_plan(text: &str, cx: &mut App) -> Option<Arc<Plan>> {
    if let Some(cache) = cx.try_global::<Cache>()
        && let Some((_, plan)) = cache.0.iter().find(|(key, _)| key == text)
    {
        return Some(plan.clone());
    }
    let plan = Arc::new(plan(text)?);
    if cx.try_global::<Cache>().is_none() {
        cx.set_global(Cache::default());
    }
    let cache = cx.global_mut::<Cache>();
    if cache.0.len() == CACHE_ENTRIES {
        cache.0.pop_front();
    }
    cache.0.push_back((text.to_owned(), plan.clone()));
    Some(plan)
}

fn inside(cell: Rect, width: f32, height: f32) -> bool {
    cell.x >= 0.0 && cell.y >= 0.0 && cell.x + cell.width <= width && cell.y + cell.height <= height
}

fn scaled(cell: Rect, scale: f32) -> Rect {
    Rect {
        x: cell.x * scale,
        y: cell.y * scale,
        width: cell.width * scale,
        height: cell.height * scale,
    }
}

fn paint_text(
    plan: &Plan,
    scale: f32,
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
) -> usize {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    // Truncation uses the real Departure ellipsis; underlying native label is intact.
    let ellipsis = plan.advance * scale > width;
    let tail = if ellipsis { plan_for_ellipsis() } else { None };
    let tail_width = tail.as_ref().map_or(0.0, |tail| tail.advance * scale);
    let text_width = (width - tail_width).max(0.0);
    let mask = window.content_mask().bounds;
    let clip = Rect {
        x: f32::from(mask.left() - bounds.left()),
        y: f32::from(mask.top() - bounds.top()),
        width: f32::from(mask.size.width),
        height: f32::from(mask.size.height),
    };
    let mut count = 0;
    window.paint_layer(bounds, |window| {
        for cell in &plan.cells {
            let cell = scaled(*cell, scale);
            if inside(cell, text_width, height) && contained(cell, clip) {
                paint_cell(cell, bounds, color, window);
                count += 1;
            }
        }
        if let Some(tail) = tail {
            for cell in &tail.cells {
                let mut cell = scaled(*cell, scale);
                cell.x += (text_width / scale).floor() * scale;
                if inside(cell, width, height) && contained(cell, clip) {
                    paint_cell(cell, bounds, color, window);
                    count += 1;
                }
            }
        }
    });
    count
}

fn contained(cell: Rect, clip: Rect) -> bool {
    cell.x >= clip.x
        && cell.y >= clip.y
        && cell.x + cell.width <= clip.x + clip.width
        && cell.y + cell.height <= clip.y + clip.height
}

fn plan_for_ellipsis() -> Option<&'static Plan> {
    use std::sync::OnceLock;
    static ELLIPSIS: OnceLock<Option<Plan>> = OnceLock::new();
    ELLIPSIS.get_or_init(|| plan("…")).as_ref()
}

fn paint_cell(cell: Rect, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    window.paint_quad(
        fill(
            Bounds::new(
                bounds.origin + point(px(cell.x), px(cell.y)),
                size(px(cell.width), px(cell.height)),
            ),
            color,
        )
        .corner_radii(px(CORNER * cell.width)),
    );
}

fn enclosure(design: BarDesign, bounds: Bounds<Pixels>, colors: &ColorTokens, window: &mut Window) {
    match design {
        BarDesign::Underline => window.paint_quad(fill(
            Bounds::new(
                point(bounds.left(), bounds.bottom() - px(1.0)),
                size(bounds.size.width, px(1.0)),
            ),
            colors.muted_foreground,
        )),
        BarDesign::Corners => {
            for (x, y, sx, sy) in [
                (bounds.left(), bounds.top(), 1.0, 1.0),
                (bounds.right(), bounds.top(), -1.0, 1.0),
                (bounds.left(), bounds.bottom(), 1.0, -1.0),
                (bounds.right(), bounds.bottom(), -1.0, -1.0),
            ] {
                let mut path = PathBuilder::stroke(px(1.0));
                let p = point(x + px(sx * 0.5), y + px(sy * 0.5));
                path.move_to(p + point(px(sx * 10.0), px(0.0)));
                path.line_to(p);
                path.line_to(p + point(px(0.0), px(sy * 10.0)));
                if let Ok(path) = path.build() {
                    window.paint_path(path, colors.muted_foreground);
                }
            }
        }
        BarDesign::Slot => window.paint_inset_shadows(
            bounds,
            px(0.0).into(),
            &[
                BoxShadow::new(px(0.0), px(3.0), colors.foreground.opacity(0.12))
                    .blur_radius(px(6.0))
                    .inset(),
            ],
        ),
        BarDesign::Notch => {
            let mut path = PathBuilder::fill();
            path.move_to(bounds.origin + point(px(10.0), px(0.0)));
            path.line_to(point(bounds.right(), bounds.top()));
            path.line_to(point(bounds.right(), bounds.bottom()));
            path.line_to(point(bounds.left(), bounds.bottom()));
            path.line_to(bounds.origin + point(px(0.0), px(10.0)));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, colors.surface);
            }
        }
        _ => {} // Material/type, no extra enclosure ink for open/mass variants.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_design_rows_build_one_hover_style_for_selected_and_unselected_states() {
        for (colors, light) in [(ColorTokens::light(), true), (ColorTokens::dark(), false)] {
            for design in BarDesign::ALL {
                let frame = Frame::new(design, colors, light);
                for selected in [false, true] {
                    let _ = frame.row(selected);
                }
            }
        }
    }

    #[test]
    fn dot_rows_use_real_departure_raster_and_separated_cells() {
        let a = plan("Back").unwrap();
        let b = plan("Local design").unwrap();
        assert!(!a.cells.is_empty());
        assert_ne!(a.cells.len(), b.cells.len());
        let glyph = atlas().unwrap().glyph('B').unwrap();
        assert_eq!(
            plan("B").unwrap().cells.len(),
            glyph.bitmap.iter().filter(|p| **p >= 128).count()
        );
        for role in [
            Role::Label,
            Role::Metadata,
            Role::Navigation,
            Role::Status,
            Role::Error,
            Role::Preview,
        ] {
            let scale = role.scale();
            for cell in &a.cells {
                let cell = scaled(*cell, scale);
                assert_eq!(cell.width, scale * (1.0 - STATIC_GAP));
                assert_eq!(cell.height, cell.width);
                assert!(cell.width < scale);
                assert!(CORNER * cell.width < cell.width / 2.0);
            }
        }
    }

    #[test]
    fn clipping_support_and_budget_never_hide_unsupported_text() {
        for text in ["café", "日本語", "RTL العربية", "abc\n", "a\t", "☃"] {
            assert!(plan(text).is_none());
        }
        assert!(plan("Asking Jev…").is_some());
        assert!(plan("preview — nothing executed · local").is_some());
        assert!(
            !atlas().unwrap().supports("—", false),
            "input guard stays frozen"
        );
        let clip = Rect {
            x: 3.0,
            y: 2.0,
            width: 7.0,
            height: 5.0,
        };
        assert!(contained(
            Rect {
                x: 3.1,
                y: 2.1,
                width: 0.75,
                height: 0.75
            },
            clip
        ));
        assert!(!contained(
            Rect {
                x: 2.9,
                y: 2.1,
                width: 0.75,
                height: 0.75
            },
            clip
        ));
        assert!(!contained(
            Rect {
                x: 9.5,
                y: 2.1,
                width: 0.75,
                height: 0.75
            },
            clip
        ));
        assert!(plan(&"a".repeat(MAX_TEXT_BYTES + 1)).is_none());
        assert!(plan(&"W".repeat(MAX_TEXT_BYTES)).is_none());
        let plan = plan("Long branch label").unwrap();
        let clipped: Vec<_> = plan
            .cells
            .iter()
            .copied()
            .filter(|c| inside(*c, 20.0, 14.0))
            .collect();
        assert!(!clipped.is_empty());
        assert!(clipped.len() < plan.cells.len());
        assert!(clipped.iter().all(|c| c.x + c.width <= 20.0));
        let spent = Cell::new(0);
        assert!(reserve(&spent, MAX_FRAME_CELLS));
        assert!(!reserve(&spent, 1));
        assert_eq!(spent.get(), MAX_FRAME_CELLS);
    }

    #[test]
    fn all_surface_roles_keep_variant_palette_identity_in_both_schemes() {
        for (colors, light) in [(ColorTokens::light(), true), (ColorTokens::dark(), false)] {
            for design in BarDesign::ALL {
                for selected in [false, true] {
                    let ink = ink(design, &colors, light, selected);
                    assert_eq!(ink.error, ink.foreground, "error uses readable foreground");
                    if design == BarDesign::DotMatrix {
                        assert_eq!(ink.background, rgb(0x000000).into());
                        assert_eq!(ink.metadata, rgb(0xcccccc).into());
                        assert_eq!(ink.selection, rgb(0x252525).into());
                    } else if design == BarDesign::Signal {
                        assert_eq!(ink.background, colors.primary);
                        assert_eq!(ink.metadata, ink.foreground);
                        if light && !selected {
                            assert_eq!(ink.foreground.a, 1.0);
                        }
                    } else if design == BarDesign::Monolith {
                        assert_eq!(ink.background, colors.foreground);
                        assert_eq!(ink.metadata, ink.foreground);
                    } else if light {
                        assert_eq!(ink.metadata, colors.foreground);
                    }
                }
            }
        }
    }
}
