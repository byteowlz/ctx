//! Template theme adapter: Studio schemes -> role colors -> native toolkit.
use gpui_kit::component::{Theme, ThemeRegistry};

pub const DARK: &str = "Lumen Dark";
pub const LIGHT: &str = "Lumen Light";

pub fn init(cx: &mut gpui_kit::App, name: &str) -> anyhow::Result<()> {
    let json = ctx_bar_design::studio_theme_set_json()?;
    ThemeRegistry::global_mut(cx).load_themes_from_str(&json)?;
    apply(cx, name);
    Ok(())
}
pub fn apply(cx: &mut gpui_kit::App, name: &str) {
    let Some(config) = ThemeRegistry::global_mut(cx).themes().get(name).cloned() else {
        return;
    };
    let mode = config.mode;
    let theme = Theme::global_mut(cx);
    if mode.is_dark() {
        theme.dark_theme = config
    } else {
        theme.light_theme = config
    }
    Theme::change(mode, None, cx);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    #[test]
    fn studio_theme_parses_with_pinned_toolkit() {
        let set: gpui_kit::component::ThemeSet =
            serde_json::from_str(&ctx_bar_design::studio_theme_set_json().unwrap()).unwrap();
        assert_eq!(set.themes.len(), 2);
        for theme in &set.themes {
            assert!(theme.colors.background.is_some());
            assert!(theme.colors.primary.is_some());
            assert!(theme.highlight.is_some());
        }
    }
}
