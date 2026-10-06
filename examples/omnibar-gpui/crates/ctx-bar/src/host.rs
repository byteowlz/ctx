//! Frameless popup policy, separate from bar decoration and OS-compositor proof.
use gpui_kit::{
    WindowBackgroundAppearance, WindowDecorations, WindowKind, WindowOptions, px, size,
};

use ctx_bar::config::Config;
use ctx_bar_design::BarDesign;

pub fn options(config: &Config, review: bool) -> WindowOptions {
    WindowOptions {
        kind: WindowKind::PopUp,
        titlebar: None,
        // Linux must not fall back to compositor-owned titlebars/borders.
        // The app draws no client-side window chrome. A compositor may ignore this.
        window_decorations: Some(WindowDecorations::Client),
        is_resizable: false,
        is_minimizable: false,
        window_min_size: Some(size(px(480.), px(64.))),
        window_background: if config.design == BarDesign::Lens {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Transparent
        },
        focus: !review,
        show: !review,
        app_id: Some("ctx-bar".into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_design_uses_frameless_popup_policy() {
        for design in BarDesign::ALL {
            for review in [false, true] {
                let config = Config {
                    design,
                    ..Default::default()
                };
                let policy = options(&config, review);
                assert_eq!(policy.kind, WindowKind::PopUp);
                assert!(policy.titlebar.is_none());
                assert_eq!(policy.window_decorations, Some(WindowDecorations::Client));
                assert!(!policy.is_resizable);
                assert!(!policy.is_minimizable);
                assert_eq!(policy.focus, !review);
                assert_eq!(policy.show, !review);
                assert!(policy.window_bounds.is_none());
            }
        }
    }

    #[test]
    fn frameless_policy_keeps_design_material_and_progressive_geometry() {
        for design in BarDesign::ALL {
            let config = Config {
                design,
                ..Default::default()
            };
            let policy = options(&config, false);
            let expected = if design == BarDesign::Lens {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            };
            assert_eq!(policy.window_background, expected);
            let idle = ctx_bar::layout::layout(design, 0, false, config.height);
            let expanded = ctx_bar::layout::layout(design, 3, false, config.height);
            assert!(idle.height <= 80.);
            assert!(expanded.height > idle.height);
            // is_resizable disables user resize handles, not Window::resize.
            assert!(expanded.height <= config.height);
        }
    }
}
