//! Explicit synthetic hidden-native proof over the real local adapter/worker/Jev.
//! No desktop capture, private corpus, execution, or focus-taking window.
#[cfg(feature = "native-review")]
#[path = "../src/app.rs"]
#[allow(dead_code)]
mod app;
#[cfg(feature = "native-review")]
#[path = "../src/host.rs"]
mod host;
#[cfg(feature = "native-review")]
#[path = "../src/matrix.rs"]
mod matrix;
#[cfg(feature = "native-review")]
#[path = "../src/presentation.rs"]
mod presentation;
#[cfg(feature = "native-review")]
#[path = "../src/theme.rs"]
mod theme;
#[cfg(not(feature = "native-review"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("requires native-review")
}
#[cfg(feature = "native-review")]
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            if theme::init(cx, "Lumen Dark").is_err() {
                std::process::exit(1);
            }
            cx.set_global(presentation::InputDiagnostics::default());
            if presentation::register_font(cx).is_err() {
                std::process::exit(1);
            }
            cx.spawn(async move |cx| {
                if let Err(error) = run(cx).await {
                    eprintln!("synthetic retrieval proof failed: {error}");
                    std::process::exit(1);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}
#[cfg(feature = "native-review")]
async fn run(cx: &mut gpui_kit::AsyncApp) -> anyhow::Result<()> {
    use ctx_bar::config::Config;
    use ctx_bar::model::{Fixture, Platform, Presentation, Request};
    use gpui_kit::{AppContext as _, EntityInputHandler as _, Keystroke};
    use std::time::{Duration, Instant};
    let directory = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("absolute synthetic proof directory required"))?;
    anyhow::ensure!(directory.is_absolute(), "absolute output required");
    std::fs::create_dir_all(&directory)?;
    // Warm the same real worker via the authenticated loopback protocol, outside UI.
    cx.background_executor()
        .spawn(async {
            let proxy = ctx_bar::proxy::Proxy::new(&ctx_bar::config::descriptor_path()?, 14)?;
            let catalog = proxy.catalog()?;
            anyhow::ensure!(
                catalog.retrieval_enabled,
                "enable retrieval in the adapter first"
            );
            let request = Request {
                query: "light mode".into(),
                platform: Platform::Macos,
                fixture: Fixture::Desktop,
                presentation: Presentation::Flat,
                node: "root".into(),
            };
            let items = proxy.retrieve(&request, &catalog)?;
            anyhow::ensure!(
                items.iter().any(|item| item.id == "appearance.light"),
                "warm local match missing"
            );
            Ok::<_, anyhow::Error>(())
        })
        .await?;
    let config = Config::default();
    let options = host::options(&config, true);
    let mut bar = None;
    let handle = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| app::Bar::new(config, false, window, cx));
        bar = Some(view.clone());
        cx.new(|cx| host::root(view, window, cx))
    })?;
    let bar = bar.ok_or_else(|| anyhow::anyhow!("missing view"))?;
    let input = bar
        .read_with(cx, |bar, _| bar.review_input())
        .ok_or_else(|| anyhow::anyhow!("missing transport input"))?;
    let mut records = Vec::new();
    let started = Instant::now();
    handle.update(cx, |_, window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "switch to light mode", window, cx)
        })
    })?;
    for phase in ["local", "jev", "catalog", "preview"] {
        println!(
            "synthetic phase-before {phase}: {}",
            bar.read_with(cx, |bar, cx| bar.review_snapshot(cx))
        );
        if phase == "catalog" {
            cx.update_window(handle.into(), |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse("f2").unwrap(), cx);
                window.draw(cx).clear(cx);
                for _ in 0..7 {
                    window.dispatch_keystroke(Keystroke::parse("down").unwrap(), cx);
                }
            })?;
        } else if phase == "preview" {
            cx.update_window(handle.into(), |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse("enter").unwrap(), cx);
            })?;
        }
        let deadline = Instant::now() + Duration::from_secs(18);
        loop {
            let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            let ready = match phase {
                "local" => {
                    snapshot["source"] == "Local Evoke"
                        && snapshot["ids"]
                            .as_array()
                            .is_some_and(|ids| !ids.is_empty())
                }
                "jev" => snapshot["source"] == "Jev via EAVS",
                "catalog" => {
                    snapshot["source"] == "Catalog browse"
                        && snapshot["ids"].as_array().is_some_and(|ids| ids.len() > 5)
                }
                _ => snapshot["receipt_source"] == "Catalog browse",
            };
            if ready {
                break;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "phase {phase} timed out: {snapshot}"
            );
            cx.background_executor()
                .timer(Duration::from_millis(5))
                .await;
        }
        let observed_ms = started.elapsed().as_secs_f64() * 1000.;
        cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })?;
        cx.background_executor()
            .timer(Duration::from_millis(60))
            .await;
        cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<()> {
            window.refresh();
            window.draw(cx).clear(cx);
            window
                .render_to_image()?
                .save(directory.join(format!("{phase}.png")))?;
            Ok(())
        })??;
        let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
        anyhow::ensure!(snapshot["timer"] == false, "unexpected timer");
        if phase == "catalog" {
            anyhow::ensure!(
                snapshot["selected"] == 7,
                "all-catalog navigation did not reach beyond five"
            );
        }
        println!("synthetic phase-after {phase}: {snapshot}");
        records.push(serde_json::json!({"phase":phase,"observed_from_synthetic_edit_ms":observed_ms,"snapshot":snapshot}));
    }
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"synthetic":true,"real_transport":true,"records":records,"limits":"Hidden synthetic native edit/loopback transport, not physical keyboard-to-present, GPU completion, accessibility or live context"}),
        )?,
    )?;
    handle.update(cx, |_, window, _| window.remove_window())?;
    Ok(())
}
