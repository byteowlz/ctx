use ctx_bar::config::Config;
use ctx_bar::model::{TreeVariant, ViewMode};
use ctx_bar_design::BarDesign;
use gpui_kit::{AppContext as _, EntityInputHandler as _};

use super::{Scene, app, host, key, save, settle, theme};

fn osascript_volume(program: &str) -> u8 {
    let output = std::process::Command::new("osascript")
        .args(["-e", program])
        .output()
        .expect("osascript volume");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("volume number")
}

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
        // Expansion must grow downward: the top edge (where the launch position
        // anchors the bar) stays put while the height changes.
        let mut edges = vec![];
        for scene in ["catalog", "idle", "catalog", "idle"] {
            handle.update(cx, |_, window, cx| {
                bar.update(cx, |bar, cx| {
                    bar.review_tree_scene(TreeVariant::Faint, scene, window, cx)
                })
            })?;
            settle(handle, cx).await?;
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            settle(handle, cx).await?;
            let bounds = cx.update_window(handle.into(), |_, window, _| window.bounds())?;
            edges.push((f32::from(bounds.origin.y), f32::from(bounds.size.height)));
        }
        // Hidden harness windows open at a default size; the first change
        // establishes the bar's own height, so measure from the second scene.
        let edges = &edges[1..];
        anyhow::ensure!(
            edges[1].1 > edges[0].1 && edges[2].1 < edges[1].1,
            "menu did not expand and collapse the window: {edges:?}"
        );
        anyhow::ensure!(
            edges.iter().all(|(top, _)| (top - edges[0].0).abs() < 0.5),
            "window top moved while the menu expanded/collapsed: {edges:?}"
        );
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
            // Real keystrokes: plain command words open the local menu without
            // routing, and Enter switches the view.
            for (word, choices, view) in [("list", 1, "list"), ("tree", 4, "tree")] {
                handle.update(cx, |_, window, cx| {
                    input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus(window, cx);
                    })
                })?;
                for ch in word.chars() {
                    key(handle, &ch.to_string(), cx)?;
                }
                settle(handle, cx).await?;
                let typed = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    typed["query"] == word
                        && typed["local_choices"] == choices
                        && typed["routable"] == false,
                    "typing {word} did not open the local command menu without routing: {typed}"
                );
                key(handle, "enter", cx)?;
                settle(handle, cx).await?;
                let applied = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    applied["view"] == view
                        && applied["input_empty"] == true
                        && applied["local_choices"].is_null(),
                    "Enter on {word} did not switch to the {view} view: {applied}"
                );
            }
            // Composing (marked) text is not committed: it neither routes nor
            // arms a timer, and Enter must not choose anything.
            handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                    input.replace_and_mark_text_in_range(None, "volume", Some(0..6), window, cx);
                })
            })?;
            settle(handle, cx).await?;
            let composing = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            anyhow::ensure!(
                composing["routable"] == false && composing["timer"] == false,
                "composing text must not route or arm a timer: {composing}"
            );
            key(handle, "enter", cx)?;
            anyhow::ensure!(
                bar.read_with(cx, |bar, cx| bar.review_snapshot(cx))["node"] == "root",
                "composition Enter chose a branch"
            );
            handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| input.unmark_text(window, cx))
            })?;
        }
        // Real execution proof, once: preview sound.volume in the tree and run
        // it with a second Enter through the app's own whitelist path.
        if scheme == "Lumen Dark" && width == 1200.0 {
            let before = osascript_volume("output volume of (get volume settings)");
            handle.update(cx, |_, window, cx| {
                bar.update(cx, |bar, cx| {
                    bar.review_tree_scene(TreeVariant::Faint, "root", window, cx)
                })
            })?;
            settle(handle, cx).await?;
            key(handle, "up", cx)?; // visual order: sound sits above the centered first branch
            settle(handle, cx).await?; // starts the pan
            cx.background_executor()
                .timer(std::time::Duration::from_millis(240))
                .await;
            settle(handle, cx).await?; // marks the tree settled
            key(handle, "enter", cx)?;
            cx.update_window(handle.into(), |_, _, cx| {
                bar.update(cx, |bar, cx| bar.review_tree_complete(cx))
            })?;
            settle(handle, cx).await?;
            let child = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            anyhow::ensure!(
                child["node"] == "fixture-sound" && child["status"] == "Ready",
                "sound branch did not load: {child}"
            );
            let position = child["ids"]
                .as_array()
                .unwrap()
                .iter()
                .position(|id| id == "sound.volume")
                .expect("sound.volume offered");
            handle.update(cx, |_, _window, cx| {
                bar.update(cx, |bar, _| bar.review_select(position))
            })?; // navigation is proven elsewhere; this scene tests execution
            cx.background_executor()
                .timer(std::time::Duration::from_millis(240))
                .await;
            settle(handle, cx).await?; // child-entry pan must settle before Enter chooses
            key(handle, "enter", cx)?;
            settle(handle, cx).await?;
            let preview = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            anyhow::ensure!(
                preview["receipt"]["item"]["id"] == "sound.volume",
                "previewed wrong item: {preview}"
            );
            anyhow::ensure!(
                preview["runnable"] == true,
                "preview is not runnable: {preview}"
            );
            key(handle, "enter", cx)?;
            let mut executed = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            for _ in 0..40 {
                if executed["execution"]
                    .as_str()
                    .is_some_and(|note| note.starts_with("Ran: volume +10"))
                {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                settle(handle, cx).await?;
                executed = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            }
            anyhow::ensure!(
                executed["execution"]
                    .as_str()
                    .is_some_and(|note| note.starts_with("Ran: volume +10")),
                "explicit Enter did not run the action: {executed}"
            );
            std::process::Command::new("osascript")
                .args(["-e", &format!("set volume output volume {before}")])
                .output()
                .expect("restore volume");
            anyhow::ensure!(
                osascript_volume("output volume of (get volume settings)") == before,
                "volume was not restored"
            );
            key(handle, "escape", cx)?;
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
