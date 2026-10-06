//! Native ctx omnibar trial. Local appearance commands and preview-only routing.
mod app;
mod host;
mod matrix;
mod presentation;
mod review;
mod theme;

use std::path::PathBuf;

use gpui_kit::component::Root;
use gpui_kit::{App, AppContext as _, AsyncApp, WindowBounds, WindowOptions, px, size};

use ctx_bar::config::Config;

const HELP: &str = "ctx-bar — native suggestion preview (nothing executes)\n\nType ctx theme to list all ten designs; ctx theme dot selects Dot matrix.\nLocal design commands never go to Jev. No separate picker or startup chrome.\n\n--config PATH\n--design underline|monolith|lens|signal|prompt|dot-matrix|corners|slot|unframed|notch\n--theme 'Lumen Dark'|'Lumen Light'\n--presentation flat|branches\n--platform macos|windows|linux|omarchy\n--fixture desktop|selection|audio|unavailable\n--auto-select-first | --no-auto-select-first\n--timeout-seconds 1..30\n--review-dir ABSOLUTE_PATH (requires native-review feature; hidden synthetic scenes, no model)\n\nFlat and timeout-off are the baseline. Credentials: local proxy descriptor only.\nNo EAVS/provider keys in this app. System dictation/accessibility remain review gates.";

fn main() -> anyhow::Result<()> {
    env_logger::try_init().ok();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let (args, review_dir) = review::extract_args(args)?;
    if review_dir.is_some() && !cfg!(feature = "native-review") {
        anyhow::bail!("Rebuild with --features native-review for --review-dir");
    }
    let config = ctx_bar::config::load_or_create(args)?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let initialized =
                theme::init(cx, &config.theme).and_then(|()| presentation::register_font(cx));
            if let Err(error) = initialized {
                fatal(error);
            }
            let options = window_options(&config, review_dir.is_some(), cx);
            cx.spawn(async move |cx| {
                if let Err(error) = open(config, review_dir, options, cx).await {
                    fatal(error);
                }
            })
            .detach();
        });
    Ok(())
}

fn fatal(error: anyhow::Error) -> ! {
    // macOS App::quit terminates with status 0 before Application::run returns.
    // In particular, a failed synthetic capture must not appear successful.
    eprintln!("Native omnibar failed: {error}");
    std::process::exit(1);
}

fn window_options(config: &Config, review: bool, cx: &App) -> WindowOptions {
    let height = ctx_bar::layout::layout(config.design, 0, false, config.height).height;
    let mut options = host::options(config, review);
    options.window_bounds = Some(WindowBounds::centered(
        size(px(config.width), px(height)),
        cx,
    ));
    options
}

async fn open(
    config: Config,
    review_dir: Option<PathBuf>,
    options: WindowOptions,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    let review = review_dir.is_some();
    let mut bar = None;
    let handle = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| app::Bar::new(config, review, window, cx));
        bar = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    })?;
    if let (Some(directory), Some(bar)) = (review_dir, bar) {
        review::capture(handle, bar, directory, cx).await?;
        cx.update(|cx| cx.quit());
    }
    Ok(())
}
