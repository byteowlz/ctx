//! Native exploratory ctx/omnibar rev1. Preview-only, no host capture or execution.
mod app;
mod theme;

use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, px, size,
};

fn main() -> anyhow::Result<()> {
    env_logger::try_init().ok();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "ctx-bar — native suggestion preview (nothing executes)\n\n--config PATH\n--theme 'Lumen Dark'|'Lumen Light'\n--platform macos|windows|linux|omarchy\n--fixture desktop|selection|audio|unavailable\n--auto-select-first | --no-auto-select-first\n--timeout-seconds 1..30\n\nFlat is the baseline; use the in-window Flat/Branches control for the exploratory A/B.\nCredentials: local proxy descriptor only. No EAVS/provider keys in this app."
        );
        return Ok(());
    }
    let config = ctx_bar::config::load_or_create(args)?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = theme::init(cx, &config.theme) {
                log::error!("Cannot load Studio theme: {error}");
                cx.quit();
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    size(px(config.width), px(config.height)),
                    cx,
                )),
                window_min_size: Some(size(px(480.), px(400.))),
                window_background: WindowBackgroundAppearance::Blurred,
                kind: WindowKind::PopUp,
                titlebar: None,
                app_id: Some("ctx-bar".into()),
                ..Default::default()
            };
            cx.spawn(async move |cx| {
                let opened = cx.open_window(options, |window, cx| {
                    let bar = cx.new(|cx| app::Bar::new(config, window, cx));
                    cx.new(|cx| Root::new(bar, window, cx))
                });
                if opened.is_err() {
                    log::error!("Cannot open native omnibar window");
                    cx.update(|cx| cx.quit());
                }
            })
            .detach();
        });
    Ok(())
}
