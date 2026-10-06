//! Native input-only Studio trial: proposal/ctx-minimal-bars 7543d7e/435b5aa.
//! Editing, IME, accessibility and authorization stay in the first-party Input.
//! Matrix paints *after* Input: only a complete current-frame mask covers its
//! ordinary ink. No opacity/color trick can make an unsupported value invisible.
use std::borrow::Cow;

use anyhow::{Result, anyhow};
use ctx_bar_design::BarDesign;
use gpui_kit::base::ColorTokens;
use gpui_kit::base::input::InputEditorStyle;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    AnyElement, App, Bounds, BoxShadow, Entity, EntityInputHandler as _, FontWeight, Global, Hsla,
    IntoElement as _, ParentElement as _, PathBuilder, Pixels, Styled as _, TextRun, Window,
    canvas, div, fill, font, point, px, rgb, size,
};

use crate::matrix::{CORNER, FONT_BYTES, FONT_FAMILY, FONT_SIZE, Ink, Mask, PITCH, Rect, atlas};

struct RegisteredDeparture;
impl Global for RegisteredDeparture {}

/// Call once on the UI thread, after component init and before opening a window.
/// An error leaves normal visible native text; the matrix never uses a substitute.
pub fn register_font(cx: &mut App) -> Result<()> {
    if cx.try_global::<RegisteredDeparture>().is_some() {
        return Ok(());
    }
    atlas().ok_or_else(|| anyhow!("embedded Departure Mono raster atlas failed to decode"))?;
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(FONT_BYTES)])?;
    if !cx
        .text_system()
        .all_font_names()
        .iter()
        .any(|name| name == FONT_FAMILY)
    {
        return Err(anyhow!(
            "Departure Mono registration did not expose its family"
        ));
    }
    cx.set_global(RegisteredDeparture);
    Ok(())
}

/// Public rendering seam. Contains exactly one real, labeled Input and decoration.
/// No startup title, selectors, badges, suggestions, loading dots or footer.
pub fn input(
    design: BarDesign,
    state: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let colors = cx.theme().color_tokens();
    let height = design.bar_height();
    let (ink, placeholder, caret) = input_ink(design, &colors, !cx.theme().is_dark());
    let padding = match design {
        BarDesign::Underline => 4.0,
        BarDesign::Unframed => 0.0,
        BarDesign::Prompt => 52.0,
        BarDesign::Notch => 26.0,
        _ => 22.0,
    };
    let text_size = match design {
        BarDesign::Monolith => 23.0,
        BarDesign::Prompt => 17.0,
        BarDesign::DotMatrix => FONT_SIZE,
        BarDesign::Slot => 18.0,
        BarDesign::Unframed => 25.0,
        _ => 20.0,
    };
    let focused = state
        .read(cx)
        .presentation()
        .focus_handle()
        .is_focused(window);
    let mut native = Input::new(state)
        .aria_label("ctx input")
        .appearance(false)
        .bordered(false)
        .focus_bordered(false)
        .cleanable(false)
        .w_full()
        .text_color(ink)
        .font_weight(FontWeight::NORMAL)
        .text_size(px(text_size))
        .line_height(px(if design == BarDesign::DotMatrix {
            42.0
        } else {
            30.0
        }))
        .pl(px(padding))
        .pr(px(if design == BarDesign::Unframed {
            0.0
        } else {
            22.0
        }))
        .py_0();
    // Input::h is multi-line-only. Set the single-line frame via Styled explicitly.
    native.style().size.height = Some(px(height).into());
    if matches!(design, BarDesign::Monolith | BarDesign::Notch) {
        native = native.font_weight(FontWeight::SEMIBOLD);
    }
    if design == BarDesign::Prompt {
        native = native.font_family(cx.theme().mono_font_family.clone());
    } else if design == BarDesign::DotMatrix {
        native = native.font_family(FONT_FAMILY);
    }

    // Component Input projects its default visual editor style in request-layout.
    // This earlier sibling's prepaint projects variant ink before native text
    // prepaint, so placeholders/caret/selection have contrast on inverted slabs.
    // No value, focus, selection, permissions, subscriptions or IME state is changed.
    let style_state = state.clone();
    let background = colors.background;
    let selection = if matches!(design, BarDesign::Monolith | BarDesign::Signal) {
        ink.opacity(0.25)
    } else if design == BarDesign::DotMatrix {
        ink.opacity(0.3)
    } else {
        cx.theme().selection
    };
    let editor_style = InputEditorStyle {
        foreground: ink,
        muted_foreground: placeholder,
        background,
        border: colors.border,
        selection,
        caret,
        highlight_styles: cx.theme().highlight_theme.clone(),
        ..Default::default()
    };
    let decoration = canvas(
        move |_, _, cx| {
            style_state.update(cx, |state, _| {
                if !state.presentation().is_multi_line() {
                    state.set_editor_style(editor_style);
                }
            });
        },
        move |bounds, _, window, _| {
            paint_enclosure(design, bounds, &colors, window);
            if focused && design != BarDesign::DotMatrix {
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(padding), bounds.bottom() - px(2.0)),
                        size(px(26.0), px(2.0)),
                    ),
                    ink,
                ));
            }
        },
    )
    .absolute()
    .size_full();
    let mut frame = div().relative().w_full().h(px(height)).flex_shrink_0();
    if design == BarDesign::Lens {
        frame = frame.rounded(px(14.0)).shadow(vec![
            BoxShadow::new(px(0.0), px(8.0), background.opacity(0.4)).blur_radius(px(18.0)),
        ]);
    }
    frame = frame.child(decoration).child(native);
    if design == BarDesign::DotMatrix {
        let state = state.clone();
        frame = frame.child(
            // Fresh native last_layout/last_bounds are written in Input's paint,
            // not prepaint. Read here, after that sibling has actually painted.
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    if let Some(mask) = native_mask(&state, bounds, window, cx) {
                        // Black is the explicit monochrome stage exception, not a
                        // solid enclosure: every visible mark remains a uniform cell.
                        window.paint_quad(fill(bounds, rgb(0x000000)));
                        paint_mask(&mask, bounds, window);
                    }
                },
            )
            .absolute()
            .size_full(),
        );
    }
    frame.into_any_element()
}

/// Placeholder ink is projected into native InputEditorStyle, not just text_color.
fn input_ink(design: BarDesign, colors: &ColorTokens, light: bool) -> (Hsla, Hsla, Hsla) {
    // Bounded native role pairings: canonical tokens themselves stay unchanged.
    let mut opaque_paper = colors.background;
    opaque_paper.a = 1.0;
    match design {
        BarDesign::Monolith => (colors.background, colors.background, colors.background),
        BarDesign::Signal if light => (opaque_paper, opaque_paper, opaque_paper),
        BarDesign::Signal => (
            colors.primary_foreground,
            colors.primary_foreground,
            colors.primary_foreground,
        ),
        BarDesign::DotMatrix => {
            let white = rgb(0xffffff).into();
            (white, white, white)
        }
        BarDesign::Underline | BarDesign::Corners | BarDesign::Unframed if light => {
            (colors.foreground, colors.foreground, colors.primary)
        }
        _ => (colors.foreground, colors.muted_foreground, colors.primary),
    }
}

fn paint_enclosure(
    design: BarDesign,
    bounds: Bounds<Pixels>,
    colors: &ColorTokens,
    window: &mut Window,
) {
    let surface = colors.surface;
    let foreground = colors.foreground;
    let background = colors.background;
    let primary = colors.primary;
    let muted = colors.muted_foreground;
    match design {
        BarDesign::Underline => window.paint_quad(fill(
            Bounds::new(
                point(bounds.left(), bounds.bottom() - px(1.0)),
                size(bounds.size.width, px(1.0)),
            ),
            muted,
        )),
        BarDesign::Monolith => window.paint_quad(fill(bounds, foreground)),
        // Native tint and rounding only; the host owns OS window blur.
        BarDesign::Lens => {
            window.paint_quad(fill(bounds, surface.opacity(0.94)).corner_radii(px(14.0)))
        }
        BarDesign::Signal => window.paint_quad(fill(bounds, primary)),
        BarDesign::Prompt => {
            window.paint_quad(fill(bounds, background));
            let p = bounds.origin + point(px(22.0), bounds.size.height / 2.0);
            let mut path = PathBuilder::stroke(px(1.25));
            path.move_to(p + point(px(0.0), px(-5.0)));
            path.line_to(p + point(px(5.0), px(0.0)));
            path.line_to(p + point(px(0.0), px(5.0)));
            path.move_to(p + point(px(7.0), px(5.0)));
            path.line_to(p + point(px(13.0), px(5.0)));
            if let Ok(path) = path.build() {
                window.paint_path(path, foreground);
            }
        }
        BarDesign::DotMatrix => {
            window.paint_quad(fill(bounds, rgb(0x000000)));
            if let Some(mask) = Mask::new(bounds.size.width.into(), bounds.size.height.into()) {
                paint_mask(&mask, bounds, window);
            }
        }
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
                    window.paint_path(path, muted);
                }
            }
        }
        BarDesign::Slot => {
            window.paint_quad(fill(bounds, background));
            window.paint_inset_shadows(
                bounds,
                px(0.0).into(),
                &[BoxShadow::new(px(0.0), px(3.0), foreground.opacity(0.12))
                    .blur_radius(px(6.0))
                    .inset()],
            );
        }
        BarDesign::Unframed => {}
        BarDesign::Notch => {
            let mut path = PathBuilder::fill();
            path.move_to(bounds.origin + point(px(10.0), px(0.0)));
            path.line_to(point(bounds.right(), bounds.top()));
            path.line_to(point(bounds.right(), bounds.bottom()));
            path.line_to(point(bounds.left(), bounds.bottom()));
            path.line_to(bounds.origin + point(px(0.0), px(10.0)));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, surface);
            }
        }
    }
}

fn local(bounds: Bounds<Pixels>, stage: Bounds<Pixels>) -> Rect {
    Rect {
        x: (bounds.origin.x - stage.origin.x).into(),
        y: (bounds.origin.y - stage.origin.y).into(),
        width: bounds.size.width.into(),
        height: bounds.size.height.into(),
    }
}

/// Return None BEFORE covering any native ink when composition/coverage/geometry
/// is unsupported. Values are ASCII LTR; each byte uses native range bounds.
/// Empty placeholders may additionally use the verified font's ellipsis glyph.
fn native_mask(
    entity: &Entity<InputState>,
    stage: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Mask> {
    cx.try_global::<RegisteredDeparture>()?;
    let composing = entity.update(cx, |state, cx| {
        state.marked_text_range(window, cx).is_some()
    });
    if composing {
        return None;
    }
    let state = entity.read(cx);
    let presentation = state.presentation();
    if presentation.is_masked()
        || presentation.is_multi_line()
        || presentation.is_loading()
        || presentation.is_disabled()
    {
        return None;
    }
    let value = state.value();
    let empty = value.is_empty();
    let text = if empty {
        presentation.placeholder().clone()
    } else {
        value
    };
    let atlas = atlas()?;
    if text.len() > 4096 || !atlas.supports(&text, empty) {
        return None;
    }
    let text_bounds = state.text_bounds()?;
    let clip = local(state.input_bounds(), stage);
    let (_, line_height) = state.cursor_layout()?;
    let font = font(FONT_FAMILY);
    let font_id = window.text_system().resolve_font(&font);
    let line = window.text_system().shape_line(
        text.clone(),
        px(FONT_SIZE),
        &[TextRun {
            len: text.len(),
            font,
            color: rgb(0xffffff).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let baseline = text_bounds.origin.y - stage.origin.y
        + (line_height - line.ascent - line.descent) / 2.0
        + line.ascent;
    let mut mask = Mask::new(stage.size.width.into(), stage.size.height.into())?;
    let selected = state.selected_range();
    if !empty && !selected.is_empty() && window.is_window_active() {
        mask.rectangle(
            local(state.range_to_bounds(&selected)?, stage),
            clip,
            Ink::Selection,
        );
    }
    for run in &line.runs {
        if run.font_id != font_id {
            return None;
        }
        for shaped in &run.glyphs {
            let ch = text.get(shaped.index..)?.chars().next()?;
            let glyph = atlas.glyph(ch)?;
            if shaped.id.0 != u32::from(glyph.index) {
                return None;
            }
            let x = if empty {
                text_bounds.origin.x + shaped.position.x - stage.origin.x
            } else {
                let native = state.range_to_bounds(&(shaped.index..shaped.index + 1))?;
                let expected = line.x_for_index(shaped.index + 1) - line.x_for_index(shaped.index);
                // A different inherited font/size or shaping result cannot be masked.
                if (native.size.width - expected).abs() > px(0.5) {
                    return None;
                }
                native.origin.x - stage.origin.x + shaped.position.x
                    - line.x_for_index(shaped.index)
            };
            mask.glyph(glyph, x.into(), (baseline + shaped.position.y).into(), clip);
        }
    }
    // show_cursor is private in gpui-base: a steady, focused native-geometry caret
    // is the explicit trial exception. Native selection/editing/blink stay intact.
    if selected.is_empty()
        && presentation.focus_handle().is_focused(window)
        && window.is_window_active()
        && !presentation.is_disabled()
    {
        let (bounds, _) = state.cursor_layout()?;
        let mut caret = local(bounds, stage);
        caret.x = (caret.x / PITCH).round() * PITCH;
        caret.width = PITCH;
        mask.rectangle(caret, clip, Ink::Caret);
    }
    Some(mask)
}

fn paint_mask(mask: &Mask, bounds: Bounds<Pixels>, window: &mut Window) {
    for (index, ink) in mask.cells.iter().enumerate() {
        let cell = mask.cell(index);
        let color = match ink {
            Ink::Unlit => 0x101010,
            Ink::Selection => 0x666666,
            Ink::SelectedLetter => 0xffffff,
            Ink::Letter | Ink::Caret => 0xffffff,
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    bounds.origin + point(px(cell.x), px(cell.y)),
                    size(px(cell.width), px(cell.height)),
                ),
                rgb(color),
            )
            .corner_radii(px(CORNER * cell.width)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_ink_tracks_each_background_role_in_both_palettes() {
        // Pure projection contract, not a claim of actual native render contrast.
        for (colors, light) in [(ColorTokens::light(), true), (ColorTokens::dark(), false)] {
            for design in BarDesign::ALL {
                let (ink, placeholder, caret) = input_ink(design, &colors, light);
                match design {
                    BarDesign::Monolith => {
                        assert_eq!(
                            (ink, placeholder, caret),
                            (colors.background, colors.background, colors.background)
                        );
                        assert_ne!(placeholder, colors.foreground);
                    }
                    BarDesign::Signal if light => {
                        let mut opaque_paper = colors.background;
                        opaque_paper.a = 1.0;
                        assert_eq!(
                            (ink, placeholder, caret),
                            (opaque_paper, opaque_paper, opaque_paper)
                        );
                    }
                    BarDesign::Underline | BarDesign::Corners | BarDesign::Unframed if light => {
                        assert_eq!(
                            (ink, placeholder, caret),
                            (colors.foreground, colors.foreground, colors.primary)
                        );
                    }
                    BarDesign::Signal => {
                        assert_eq!(
                            (ink, placeholder, caret),
                            (
                                colors.primary_foreground,
                                colors.primary_foreground,
                                colors.primary_foreground
                            )
                        );
                        assert_ne!(placeholder, colors.primary);
                    }
                    BarDesign::DotMatrix => {
                        let white: Hsla = rgb(0xffffff).into();
                        assert_eq!((ink, placeholder, caret), (white, white, white));
                    }
                    _ => {
                        assert_eq!(
                            (ink, placeholder, caret),
                            (colors.foreground, colors.muted_foreground, colors.primary)
                        );
                    }
                }
            }
        }
    }
}
