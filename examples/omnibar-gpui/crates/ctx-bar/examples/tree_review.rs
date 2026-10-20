//! Hidden synthetic GPUI scene pixels and native keyboard routing; no host capture.
#[cfg(feature = "native-review")]
use gpui_kit::AppContext as _;
#[cfg(feature = "native-review")]
#[path = "../src/app.rs"]
#[allow(dead_code)]
mod app;
#[cfg(feature = "native-review")]
#[path = "tree_review/extra.rs"]
mod extra_proof;
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
#[cfg(feature = "native-review")]
#[path = "tree_review/timing.rs"]
mod timing;

#[cfg(not(feature = "native-review"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("tree_review requires --features native-review")
}

#[cfg(feature = "native-review")]
fn main() -> anyhow::Result<()> {
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("supply an absolute proof directory"))?,
    );
    anyhow::ensure!(directory.is_absolute(), "absolute proof directory required");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = presentation::register_font(cx) {
                fatal(error);
            }
            cx.spawn(async move |cx| {
                if let Err(error) = capture(directory, cx).await {
                    fatal(error);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    Ok(())
}
#[cfg(feature = "native-review")]
fn fatal(error: anyhow::Error) -> ! {
    eprintln!("native tree review failed: {error:#}");
    std::process::exit(1)
}

#[cfg(feature = "native-review")]
async fn capture(directory: std::path::PathBuf, cx: &mut gpui_kit::AsyncApp) -> anyhow::Result<()> {
    use ctx_bar::config::Config;
    use ctx_bar::model::{TreeVariant, ViewMode};
    use ctx_bar_design::BarDesign;
    use gpui_kit::Keystroke;
    let mut captures = Vec::new();
    let mut keyboard_checks = 0;
    for (scheme, width) in [("Lumen Dark", 680.0), ("Lumen Light", 480.0)] {
        cx.update(|cx| theme::init(cx, scheme))?;
        for design in BarDesign::ALL {
            let config = Config {
                design,
                width,
                view: ViewMode::Tree,
                reduce_motion: true,
                ..Default::default()
            };
            let mut bar = None;
            let handle = cx.open_window(host::options(&config, true), |window, cx| {
                let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
                bar = Some(view.clone());
                cx.new(|cx| host::root(view, window, cx))
            })?;
            let bar = bar.ok_or_else(|| anyhow::anyhow!("review view unavailable"))?;
            for variant in TreeVariant::ALL {
                for scene in ["root", "child", "preview", "back"] {
                    handle.update(cx, |_, window, cx| {
                        bar.update(cx, |bar, cx| {
                            bar.review_tree_scene(*variant, scene, window, cx)
                        })
                    })?;
                    settle(handle, cx).await?;
                    let snapshot = save(
                        &directory,
                        Scene {
                            scheme,
                            design,
                            width,
                            variant: *variant,
                            name: scene,
                        },
                        handle,
                        &bar,
                        cx,
                    )?;
                    anyhow::ensure!(snapshot["timer"] == false, "fixture armed a timer");
                    captures.push(snapshot);
                }
                handle.update(cx, |_, window, cx| {
                    bar.update(cx, |bar, cx| {
                        bar.review_tree_scene(*variant, "root", window, cx)
                    })
                })?;
                settle(handle, cx).await?;
                cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<()> {
                    window.dispatch_keystroke(Keystroke::parse("down")?, cx);
                    window.draw(cx).clear(cx);
                    Ok(())
                })??;
                settle(handle, cx).await?;
                let panned = save(
                    &directory,
                    Scene {
                        scheme,
                        design,
                        width,
                        variant: *variant,
                        name: "panned",
                    },
                    handle,
                    &bar,
                    cx,
                )?;
                anyhow::ensure!(panned["selected"] == 2, "Down did not follow visual order");
                captures.push(panned);
                // Real native Enter -> one branch request, then a synthetic fresh reply.
                handle.update(cx, |_, window, cx| {
                    bar.update(cx, |bar, cx| {
                        bar.review_tree_scene(*variant, "root", window, cx)
                    })
                })?;
                settle(handle, cx).await?;
                key(handle, "enter", cx)?;
                let pending = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    pending["node"] == "fixture-appearance" && pending["status"] == "Loading",
                    "Enter did not request exactly one child: {pending}"
                );
                handle.update(cx, |_, _, cx| {
                    bar.update(cx, |bar, cx| bar.review_tree_complete(cx))
                })?;
                settle(handle, cx).await?;
                key(handle, "enter", cx)?;
                settle(handle, cx).await?;
                let preview = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    preview["status"]
                        .as_str()
                        .is_some_and(|status| status.starts_with("Preview("))
                        && preview["timer"] == false,
                    "Enter did not stop at inert preview"
                );
                // View switching is a ctx command, not a function key: apply it
                // through the real input path and expect the usual command reset.
                let input = bar.read_with(cx, |bar, _| bar.review_input().unwrap());
                handle.update(cx, |_, window, cx| {
                    input.update(cx, |input, cx| {
                        input.focus(window, cx);
                        // set_value is event-silent by design; the app state
                        // mirrors Change events, so drive it like a user edit.
                        input.replace_all("ctx view list", window, cx)
                    })
                })?;
                settle(handle, cx).await?;
                let offered = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    offered["local_choices"] == 1,
                    "ctx view list did not offer exactly one view action: {offered}"
                );
                key(handle, "enter", cx)?;
                settle(handle, cx).await?;
                let list = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                anyhow::ensure!(
                    list["view"] == "list"
                        && list["query"] == ""
                        && list["input_empty"] == true
                        && list["local_choices"].is_null()
                        && list["timer"] == false,
                    "ctx view switch left stale input or menu: {list}"
                );
                keyboard_checks += 3;
            }
            cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
        }
    }
    let system_reduce_motion = cx.update(|cx| cx.reduce_motion());
    captures.extend(extra_proof::capture(&directory, cx).await?);
    timing::run(&directory, cx).await?;
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "limits":"Synthetic declarations and simulated decision replies. Hidden GPUI pixels/native dispatch, not live inference, host discovery, physical input, compositor completion, OS control execution or screen-reader qualification.",
            "captures":captures,"keyboard_checks":keyboard_checks,
            "pixel_proof":"dot-separation probes require a >=2x capture; on lower-scale displays they print a skip instead of asserting","system_reduce_motion_at_start":system_reduce_motion,"animation_fixture_override":"App-only false for supplemental hidden animation/IME poses, restored afterwards; OS preferences unchanged"
        }))?,
    )?;
    println!(
        "native tree proof: {} scenes; {keyboard_checks} native keyboard checks",
        captures.len()
    );
    Ok(())
}

#[cfg(feature = "native-review")]
async fn settle(
    handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<()> {
    use std::time::Duration;
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })?;
    cx.background_executor()
        .timer(Duration::from_millis(45))
        .await;
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })?;
    Ok(())
}
#[cfg(feature = "native-review")]
fn key(
    handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
    key: &str,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<()> {
    cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<()> {
        window.dispatch_keystroke(gpui_kit::Keystroke::parse(key)?, cx);
        Ok(())
    })??;
    Ok(())
}
#[cfg(feature = "native-review")]
struct Scene<'a> {
    scheme: &'a str,
    design: ctx_bar_design::BarDesign,
    width: f32,
    variant: ctx_bar::model::TreeVariant,
    name: &'a str,
}
#[cfg(feature = "native-review")]
fn save(
    directory: &std::path::Path,
    scene: Scene<'_>,
    handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
    bar: &gpui_kit::Entity<app::Bar>,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<serde_json::Value> {
    let Scene {
        scheme,
        design,
        width,
        variant,
        name: scene,
    } = scene;
    let variant = serde_json::to_value(variant)?
        .as_str()
        .unwrap_or("unknown")
        .to_owned();
    let dir = directory
        .join(if scheme == "Lumen Dark" {
            "dark"
        } else {
            "light"
        })
        .join(&variant)
        .join(format!("{width:.0}"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}-{scene}.png", design.id()));
    cx.update(|cx| cx.set_global(presentation::InputDiagnostics::default()));
    let mut snapshot =
        cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<_> {
            window.refresh();
            window.draw(cx).clear(cx);
            let image = window.render_to_image()?;
            let snapshot = bar.read(cx).review_snapshot(cx);
            image.save(&path)?;
            if snapshot["tree"]["viewport_top"].is_number() {
                verify_floating(&snapshot, design, width, image.width(), |x, y| {
                    image.get_pixel(x, y).0
                })?;
            }
            if design == ctx_bar_design::BarDesign::DotMatrix
                && snapshot["tree"]["selected_center"].is_array()
                && snapshot["tree_moving"] == false
            {
                verify_grid(&snapshot, width, image.width(), |x, y| {
                    image.get_pixel(x, y).0
                })?;
            }
            Ok(snapshot)
        })??;
    snapshot["file"] = serde_json::json!(path.strip_prefix(directory)?.to_string_lossy());
    snapshot["design"] = serde_json::json!(design.id());
    snapshot["scheme"] = serde_json::json!(scheme);
    snapshot["variant"] = serde_json::json!(variant);
    snapshot["scene"] = serde_json::json!(scene);
    Ok(snapshot)
}

/// Tree view anatomy over the desktop: nothing is painted beside the bar, a
/// dark stage sits behind the tree, and the stage has no window-shaped edge
/// (the window's outer corner stays clear).
#[cfg(feature = "native-review")]
fn verify_floating(
    snapshot: &serde_json::Value,
    design: ctx_bar_design::BarDesign,
    width: f32,
    pixel_width: u32,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) -> anyhow::Result<()> {
    let tree = &snapshot["tree"];
    let window_width = tree["window_width"].as_f64().map_or(width, |v| v as f32);
    let bar_width = tree["bar_width"].as_f64().map_or(width, |v| v as f32);
    let top = tree["viewport_top"].as_f64().unwrap_or(0.0) as f32;
    let scale = pixel_width as f32 / window_width;
    let alpha = |x: f32, y: f32| pixel((x * scale) as u32, (y * scale) as u32)[3];
    // Painted grounds are 200+ alpha; a design's own soft input shadow is not.
    if window_width > bar_width + 40.0 {
        let beside = alpha(
            (window_width - bar_width) / 2.0 - 12.0,
            design.bar_height() / 2.0,
        );
        anyhow::ensure!(
            beside < 64,
            "{}: area beside the bar is painted (alpha {beside})",
            design.id()
        );
    }
    let stage = alpha(64.0, top + 48.0);
    anyhow::ensure!(
        stage >= 150,
        "{}: tree stage missing (alpha {stage})",
        design.id()
    );
    let corner = alpha(2.0, top + 2.0);
    anyhow::ensure!(
        corner < 64,
        "{}: stage reaches the window edge (alpha {corner})",
        design.id()
    );
    Ok(())
}

#[cfg(feature = "native-review")]
fn verify_grid(
    snapshot: &serde_json::Value,
    width: f32,
    pixel_width: u32,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) -> anyhow::Result<()> {
    let center = snapshot["tree"]["selected_center"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("selected center unavailable"))?;
    let x = center[0].as_f64().unwrap() as f32;
    let y = center[1].as_f64().unwrap() as f32;
    let expected_x = x - 132.0 + 6.0;
    let bounds = snapshot["surface_diagnostics"]["label_bounds"]
        .as_array()
        .unwrap();
    let label = bounds
        .iter()
        .find(|b| {
            (b[0].as_f64().unwrap() as f32 - expected_x).abs() < 2.0
                && (b[1].as_f64().unwrap() as f32 - y).abs() < 48.0
                && b[3].as_f64().unwrap() == 28.0
                && b[4].as_f64().unwrap_or(0.0) == 2.0
        })
        .ok_or_else(|| anyhow::anyhow!("selected title bounds missing: {snapshot}"))?;
    let window_width = snapshot["tree"]["window_width"]
        .as_f64()
        .map_or(width, |value| value as f32);
    let scale = pixel_width as f32 / window_width;
    if scale < 1.95 {
        // A 1x capture cannot resolve the gap between 2-device-pixel cells, so
        // it cannot prove separation either way. Record the skip, never a pass.
        println!("dot grid separation: skipped (capture scale {scale:.2})");
        return Ok(());
    }
    let origin_x = label[0].as_f64().unwrap() as f32;
    let origin_y = label[1].as_f64().unwrap() as f32;
    let w = label[2].as_f64().unwrap() as usize;
    let h = label[3].as_f64().unwrap() as usize;
    let lit = |x: f32, y: f32| {
        let p = pixel((x * scale).round() as u32, (y * scale).round() as u32);
        p[3] > 200 && p[..3].iter().all(|c| *c > 210)
    };
    let mut centers = 0;
    let mut gaps = 0;
    for dy in (0..h.saturating_sub(2)).step_by(2) {
        for dx in (0..w.saturating_sub(2)).step_by(2) {
            let px = origin_x + dx as f32;
            let py = origin_y + dy as f32;
            if lit(px + 1.0, py + 1.0) {
                centers += 1;
            }
            if lit(px + 1.9, py + 1.0) || lit(px + 1.0, py + 1.9) {
                gaps += 1;
            }
        }
    }
    anyhow::ensure!(
        centers > 80 && gaps == 0,
        "selected title pixels are not a visible dot grid: centers={centers}, gaps={gaps}, bounds={label}"
    );
    Ok(())
}
