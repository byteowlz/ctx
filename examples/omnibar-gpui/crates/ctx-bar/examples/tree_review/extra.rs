use ctx_bar::config::Config;
use ctx_bar::model::{TreeVariant, ViewMode};
use ctx_bar_design::BarDesign;
use gpui_kit::{AppContext as _, EntityInputHandler as _};

use super::{Scene, app, host, key, save, settle, theme};

pub(super) async fn capture(
    directory: &std::path::Path,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let native_reduce_motion = cx.update(|cx| cx.reduce_motion());
    let mut captures = Vec::new();
    for (scheme, width) in [
        ("Lumen Dark", 1200.0),
        ("Lumen Light", 1200.0),
        ("Lumen Dark", 480.0),
    ] {
        cx.update(|cx| theme::init(cx, scheme))?;
        cx.update(|cx| cx.set_reduce_motion(false)); // Synthetic override in this hidden harness only; never changes OS preferences.
        let config = Config {
            width,
            view: ViewMode::Tree,
            reduce_motion: false,
            ..Default::default()
        };
        let mut bar = None;
        let handle = cx.open_window(host::options(&config, true), |window, cx| {
            let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
            bar = Some(view.clone());
            cx.new(|cx| host::root(view, window, cx))
        })?;
        let bar = bar.ok_or_else(|| anyhow::anyhow!("view unavailable"))?;
        for variant in TreeVariant::ALL {
            for scene in [
                "catalog",
                "loading",
                "child-loading",
                "error",
                "none",
                "unicode",
                "idle",
            ] {
                handle.update(cx, |_, window, cx| {
                    bar.update(cx, |bar, cx| {
                        bar.review_tree_scene(*variant, scene, window, cx)
                    })
                })?;
                settle(handle, cx).await?;
                let snapshot = save(
                    directory,
                    Scene {
                        scheme,
                        width,
                        design: BarDesign::DotMatrix,
                        variant: *variant,
                        name: scene,
                    },
                    handle,
                    &bar,
                    cx,
                )?;
                anyhow::ensure!(
                    snapshot["timer"] == false,
                    "non-decision fixture armed dwell"
                );
                if scene == "catalog" {
                    anyhow::ensure!(
                        snapshot["ids"].as_array().unwrap().len()
                            == snapshot["catalog_count"].as_u64().unwrap() as usize,
                        "F2 hard-filtered offered scope"
                    );
                }
                if scene == "unicode" || width == 1200.0 {
                    anyhow::ensure!(
                        snapshot["input_diagnostics"]["mask_successes"] == 0,
                        "native fallback was covered"
                    );
                }
                captures.push(snapshot);
            }
            // Non-reduced actual native transition: stationary aperture, moving nodes.
            handle.update(cx, |_, window, cx| {
                bar.update(cx, |bar, cx| {
                    bar.review_tree_scene(*variant, "root", window, cx)
                })
            })?;
            settle(handle, cx).await?;
            key(handle, "down", cx)?;
            cx.update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })?;
            let moving = save(
                directory,
                Scene {
                    scheme,
                    width,
                    design: BarDesign::DotMatrix,
                    variant: *variant,
                    name: "moving",
                },
                handle,
                &bar,
                cx,
            )?;
            anyhow::ensure!(
                moving["tree_moving"] == true && moving["tree_settled"] == false,
                "native pan did not start: {moving}"
            );
            key(handle, "enter", cx)?;
            anyhow::ensure!(
                bar.read_with(cx, |bar, cx| bar.review_snapshot(cx))["node"] == "root",
                "Enter advanced an unsettled tree"
            );
            cx.background_executor()
                .timer(std::time::Duration::from_millis(240))
                .await;
            settle(handle, cx).await?;
            let settled = save(
                directory,
                Scene {
                    scheme,
                    width,
                    design: BarDesign::DotMatrix,
                    variant: *variant,
                    name: "settled",
                },
                handle,
                &bar,
                cx,
            )?;
            anyhow::ensure!(
                settled["tree_moving"] == false
                    && settled["tree_settled"] == true
                    && moving["tree"]["center"] == settled["tree"]["center"],
                "aperture moved or transition never settled"
            );
            captures.extend([moving, settled]);
            key(handle, "escape", cx)?;
            anyhow::ensure!(
                bar.read_with(cx, |bar, cx| bar.review_snapshot(cx))["query"]
                    == "adjust appearance",
                "Escape destroyed the tree request"
            );
            let input = bar.read_with(cx, |bar, _| bar.review_input().unwrap());
            handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.replace_and_mark_text_in_range(None, "abc", Some(0..3), window, cx);
                })
            })?;
            key(handle, "enter", cx)?;
            anyhow::ensure!(
                bar.read_with(cx, |bar, cx| bar.review_snapshot(cx))["node"] == "root",
                "composition Enter chose a branch"
            );
            handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| input.unmark_text(window, cx))
            })?;
        }
        captures.push(deep_list_catalog(directory, scheme, width, handle, &bar, cx).await?);
        cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
    }
    cx.update(|cx| cx.set_reduce_motion(native_reduce_motion));
    Ok(captures)
}

/// Classic list F2 over the whole expanded scope: deep rows must still draw Dot
/// text, not a spacer or native fallback, after keyboard scrolling.
async fn deep_list_catalog(
    directory: &std::path::Path,
    scheme: &'static str,
    width: f32,
    handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
    bar: &gpui_kit::Entity<app::Bar>,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<serde_json::Value> {
    handle.update(cx, |_, window, cx| {
        bar.update(cx, |bar, cx| {
            bar.review_tree_scene(TreeVariant::Faint, "list-catalog", window, cx)
        })
    })?;
    settle(handle, cx).await?;
    for _ in 0..60 {
        key(handle, "down", cx)?;
    }
    settle(handle, cx).await?;
    let snapshot = save(
        directory,
        Scene {
            scheme,
            width,
            design: BarDesign::DotMatrix,
            variant: TreeVariant::Faint,
            name: "list-catalog-deep",
        },
        handle,
        bar,
        cx,
    )?;
    let count = snapshot["ids"].as_array().map_or(0, Vec::len);
    anyhow::ensure!(
        snapshot["view"] == "list"
            && count > 60
            && count as u64 == snapshot["catalog_count"].as_u64().unwrap_or(0),
        "F2 list did not offer the whole scope: {count}"
    );
    anyhow::ensure!(
        snapshot["selected"] == 60 && snapshot["timer"] == false,
        "deep keyboard selection failed"
    );
    anyhow::ensure!(
        snapshot["surface_diagnostics"]["native_fallbacks"] == 0,
        "deep rows exhausted the Dot budget"
    );
    let viewport = 64.0..(64.0 + 5.0 * ctx_bar::layout::ROW_HEIGHT + 64.0);
    let visible = snapshot["surface_diagnostics"]["label_bounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| {
            b[1].as_f64()
                .is_some_and(|y| viewport.contains(&(y as f32)))
        })
        .count();
    anyhow::ensure!(
        visible >= 5,
        "scrolled viewport shows spacers instead of Dot rows: {visible}"
    );
    Ok(snapshot)
}
