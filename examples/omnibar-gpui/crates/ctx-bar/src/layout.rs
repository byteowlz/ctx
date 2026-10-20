//! Pure progressive-disclosure geometry; an idle launcher is only its input.
use ctx_bar_design::BarDesign;

// No outer window gutter: only the selected bar's own geometry is painted.
pub const INSET: f32 = 0.0;
pub const ROW_HEIGHT: f32 = 56.0;
pub const STATUS_HEIGHT: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub height: f32,
    pub list_height: f32,
}

pub fn layout(design: BarDesign, choices: usize, status: bool, maximum: f32) -> Layout {
    let input = design.bar_height() + INSET * 2.0;
    let status_height = if status { STATUS_HEIGHT } else { 0.0 };
    let available = (maximum - input - status_height - INSET).max(0.0);
    let list_height = (choices.min(5) as f32 * ROW_HEIGHT).min(available);
    Layout {
        height: input + status_height + list_height + if choices > 0 { INSET } else { 0.0 },
        list_height,
    }
}

/// Rows that get real text this frame. Others are equal-height spacers, so a
/// large offered scope keeps every index scrollable without spending the
/// per-frame Dot budget on rows nobody can see. The selected row's neighbours
/// stay live because keyboard `scroll_to_item` settles after render.
pub fn live_row(index: usize, selected: usize, offset_y: f32, height: f32) -> bool {
    let top = (-offset_y / ROW_HEIGHT).floor().max(0.0) as usize;
    let visible = (height / ROW_HEIGHT).ceil() as usize;
    let band = top.saturating_sub(1)..=top + visible + 1;
    band.contains(&index) || index.abs_diff(selected) <= 5
}

/// Fraction of the visible display height above the bar's top edge. Launchers
/// sit high so the menu has room to grow downward without covering the middle.
pub const LAUNCH_TOP_FRACTION: f32 = 0.2;

/// Top-left launch position inside a display's visible area (menu bar and dock
/// excluded): centred horizontally, top edge at LAUNCH_TOP_FRACTION, and moved
/// up only as far as needed so the fully expanded menu stays on screen.
pub fn launch_origin(visible: [f32; 4], width: f32, max_height: f32) -> (f32, f32) {
    let [x, y, w, h] = visible;
    let left = x + (w - width).max(0.0) / 2.0;
    let preferred = y + h * LAUNCH_TOP_FRACTION;
    let lowest = y + (h - max_height).max(0.0);
    (left, preferred.min(lowest).max(y))
}

/// Reserve the same path/status/control bands at every tree depth so the
/// aperture never shifts when a branch or final receipt is disclosed.
pub fn tree_layout(design: BarDesign, maximum: f32) -> Layout {
    let bands = design.bar_height() + STATUS_HEIGHT * 3.0;
    let list_height = (maximum - bands).clamp(64.0, 320.0);
    Layout {
        height: bands + list_height,
        list_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_idle_design_is_only_its_bar() {
        for design in BarDesign::ALL {
            let geometry = layout(design, 0, false, 520.0);
            assert_eq!(geometry.height, design.bar_height() + INSET * 2.0);
            assert_eq!(geometry.list_height, 0.0);
            assert_eq!(geometry.height, design.bar_height());
            assert!(geometry.height <= 64.0);
        }
    }

    #[test]
    fn disclosures_expand_without_a_permanent_footer() {
        let idle = layout(BarDesign::DotMatrix, 0, false, 520.0);
        let loading = layout(BarDesign::DotMatrix, 0, true, 520.0);
        let ready = layout(BarDesign::DotMatrix, 3, false, 520.0);
        assert_eq!(loading.height - idle.height, STATUS_HEIGHT);
        assert_eq!(ready.list_height, 3.0 * ROW_HEIGHT);
        assert_eq!(layout(BarDesign::DotMatrix, 0, false, 520.0), idle);
    }

    #[test]
    fn launch_sits_high_and_keeps_the_expanded_menu_on_screen() {
        // 1080p display, 25px menu bar.
        let (x, y) = launch_origin([0.0, 25.0, 1920.0, 1055.0], 680.0, 520.0);
        assert_eq!(x, 620.0);
        assert_eq!(y, 25.0 + 1055.0 * LAUNCH_TOP_FRACTION);
        assert!(y + 520.0 <= 1080.0);
        // Short display: lifted so the full menu still fits.
        let (_, y) = launch_origin([0.0, 25.0, 1280.0, 600.0], 680.0, 520.0);
        assert_eq!(y, 25.0 + 80.0);
        // Never above the visible area, never off the left edge.
        let (x, y) = launch_origin([100.0, 25.0, 400.0, 300.0], 680.0, 520.0);
        assert_eq!((x, y), (100.0, 25.0));
    }

    #[test]
    fn large_scopes_render_a_bounded_live_window() {
        let height = ROW_HEIGHT * 5.0;
        for (selected, offset) in [
            (0, 0.0),
            (40, -40.0 * ROW_HEIGHT),
            (0, -60.0 * ROW_HEIGHT),
            (87, -83.0 * ROW_HEIGHT),
        ] {
            let live: Vec<_> = (0..88)
                .filter(|index| live_row(*index, selected, offset, height))
                .collect();
            assert!(live.contains(&selected));
            assert!(live.len() <= 8 + 11, "{selected} {offset}: {}", live.len());
            let top = (-offset / ROW_HEIGHT) as usize;
            assert!((top..(top + 5).min(88)).all(|index| live.contains(&index)));
        }
    }

    #[test]
    fn ten_local_designs_scroll_inside_a_bounded_window() {
        for maximum in [240.0, 520.0, 900.0] {
            for design in BarDesign::ALL {
                let geometry = layout(design, 10, true, maximum);
                assert!(geometry.height <= maximum);
                assert!(geometry.list_height > 0.0);
                assert!(geometry.list_height <= ROW_HEIGHT * 5.0);
            }
        }
    }
}
