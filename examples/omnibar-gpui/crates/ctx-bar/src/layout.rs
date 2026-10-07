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
