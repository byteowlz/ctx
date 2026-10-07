//! Opt-in hidden native whole-surface fixtures; never a desktop/compositor capture.
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
    anyhow::bail!("surface_review requires --features native-review")
}

#[cfg(feature = "native-review")]
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2 && args[0] == "--review-dir",
        "use --review-dir ABSOLUTE_PATH"
    );
    let directory = std::path::PathBuf::from(&args[1]);
    anyhow::ensure!(directory.is_absolute(), "review directory must be absolute");
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
    eprintln!("surface review failed: {error}");
    std::process::exit(1)
}

#[cfg(feature = "native-review")]
async fn capture(directory: std::path::PathBuf, cx: &mut gpui_kit::AsyncApp) -> anyhow::Result<()> {
    use ctx_bar::config::Config;
    use ctx_bar_design::BarDesign;
    use gpui_kit::{AppContext as _, WindowBounds, px, size};
    use std::time::Duration;
    let mut captures = Vec::new();
    for (scheme, width) in [("Lumen Dark", 680.0), ("Lumen Light", 480.0)] {
        cx.update(|cx| theme::init(cx, scheme))?;
        for design in BarDesign::ALL {
            let config = Config {
                design,
                width,
                ..Default::default()
            };
            let mut options = host::options(&config, true);
            options.window_bounds =
                Some(cx.update(|cx| {
                    WindowBounds::centered(size(px(width), px(design.bar_height())), cx)
                }));
            let mut bar = None;
            let handle = cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
                bar = Some(view.clone());
                cx.new(|cx| host::root(view, window, cx))
            })?;
            let bar = bar.ok_or_else(|| anyhow::anyhow!("review view unavailable"))?;
            for scene in app::surface_review::Scene::ALL {
                handle.update(cx, |_, window, cx| {
                    bar.update(cx, |bar, cx| {
                        bar.review_surface_scene(design, scene.id(), window, cx)
                    })
                })?;
                cx.update_window(handle.into(), |_, window, cx| {
                    window.draw(cx).clear(cx);
                })?;
                cx.background_executor()
                    .timer(Duration::from_millis(60))
                    .await;
                cx.update(|cx| cx.set_global(presentation::InputDiagnostics::default()));
                let dir = directory.join(if scheme == "Lumen Dark" {
                    "dark"
                } else {
                    "light"
                });
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(format!("{}-{}.png", design.id(), scene.id()));
                let dimensions =
                    cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<_> {
                        window.refresh();
                        window.draw(cx).clear(cx);
                        let image = window.render_to_image()?;
                        image.save(&path)?;
                        verify_pixels(
                            image.width(),
                            image.height(),
                            design,
                            scene.id(),
                            width,
                            &bar.read(cx).review_snapshot(cx)["surface_diagnostics"]["label_bounds"],
                            |x, y| image.get_pixel(x, y).0,
                        )?;
                        Ok([image.width(), image.height()])
                    })??;
                let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                if design == BarDesign::DotMatrix && scene.id() != "empty" {
                    anyhow::ensure!(
                        snapshot["surface_diagnostics"]["visible_cells"]
                            .as_u64()
                            .unwrap_or(0)
                            > 0,
                        "no visible static cells"
                    );
                    let expected_fallbacks = u64::from(scene.id() == "unicode");
                    anyhow::ensure!(
                        snapshot["surface_diagnostics"]["native_fallbacks"] == expected_fallbacks,
                        "static fallback mismatch: {snapshot}"
                    );
                }
                captures.push(serde_json::json!({"scheme":scheme,"width":width,"design":design.id(),"scene":scene.id(),"file":path.strip_prefix(&directory)?.to_string_lossy(),"pixels":dimensions,"snapshot":snapshot}));
                if scene.id() == "local"
                    && matches!(
                        design,
                        BarDesign::Corners | BarDesign::Slot | BarDesign::Underline
                    )
                {
                    let before_snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                    let before =
                        cx.update_window(handle.into(), |_, window, _| window.render_to_image())??;
                    cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<()> {
                        for _ in 0..8 {
                            window.dispatch_keystroke(gpui_kit::Keystroke::parse("down")?, cx);
                        }
                        window.draw(cx).clear(cx);
                        Ok(())
                    })??;
                    cx.background_executor()
                        .timer(Duration::from_millis(60))
                        .await;
                    let scroll_path = dir.join(format!("{}-local-scrolled.png", design.id()));
                    cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<()> {
                        window.refresh();
                        window.draw(cx).clear(cx);
                        let after = window.render_to_image()?;
                        anyhow::ensure!(
                            before.dimensions() == after.dimensions(),
                            "scroll changed enclosure bounds"
                        );
                        after.save(&scroll_path)?;
                        Ok(())
                    })??;
                    let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                    anyhow::ensure!(
                        snapshot["selected"] == 8,
                        "native Down did not navigate visible local menu"
                    );
                    anyhow::ensure!(
                        snapshot["surface_diagnostics"]["enclosure_bounds"]
                            == before_snapshot["surface_diagnostics"]["enclosure_bounds"],
                        "scroll moved enclosure paint bounds"
                    );
                    captures.push(serde_json::json!({"scheme":scheme,"width":width,"design":design.id(),"scene":"local-scrolled","file":scroll_path.strip_prefix(&directory)?.to_string_lossy(),"pixels":dimensions,"snapshot":snapshot,"scroll_before":before_snapshot}));
                }
            }
            cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
        }
    }
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"kind":"actual-hidden-GPUI-scene-trial-not-approval","synthetic":true,"model_calls":false,"foreground":false,"desktop_capture":false,"captures":captures}),
        )?,
    )?;
    Ok(())
}

#[cfg(feature = "native-review")]
fn verify_pixels(
    pixel_width: u32,
    pixel_height: u32,
    design: ctx_bar_design::BarDesign,
    scene: &str,
    width: f32,
    label_bounds: &serde_json::Value,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) -> anyhow::Result<()> {
    use ctx_bar_design::BarDesign;
    if design == BarDesign::Unframed && scene == "empty" {
        anyhow::ensure!(pixel(0, 0)[3] == 0, "root surround regressed");
    }
    if design != BarDesign::DotMatrix || scene == "empty" {
        return Ok(());
    }
    let device = pixel_width as f32 / width;
    let bright = ((64.0 * device) as u32 + 1..pixel_height)
        .flat_map(|y| (0..pixel_width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = pixel(x, y);
            p[0] > 180 && p[1] > 180 && p[2] > 180
        })
        .count();
    anyhow::ensure!(bright > 30, "expanded surface ink not visible");
    if scene != "tools" {
        return Ok(());
    }
    let actual = label_bounds
        .get(0)
        .ok_or_else(|| anyhow::anyhow!("first row label bounds unavailable"))?;
    let label_x = actual[0]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("label x unavailable"))? as f32;
    let label_y = actual[1]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("label y unavailable"))? as f32;
    let label_scale = actual[4]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("label scale unavailable"))? as f32;
    let mut advance: f32 = 0.0;
    let (mut centers, mut gaps, mut total) = (0, 0, 0);
    for ch in "Switch to light mode".chars() {
        let glyph = matrix::atlas()
            .and_then(|atlas| atlas.glyph(ch))
            .ok_or_else(|| anyhow::anyhow!("review glyph unavailable"))?;
        let m = glyph.metrics;
        for (i, coverage) in glyph.bitmap.iter().enumerate() {
            if *coverage < 128 {
                continue;
            }
            let x = advance.round() + m.xmin as f32 + (i % m.width) as f32;
            let y = 11.0 - m.ymin as f32 - m.height as f32 + (i / m.width) as f32;
            let sample = |dx: f32| {
                pixel(
                    ((label_x + (x + dx) * label_scale) * device) as u32,
                    ((label_y + (y + 0.5) * label_scale) * device) as u32,
                )[0]
            };
            centers += usize::from(sample(0.5) > 220);
            gaps += usize::from(sample(0.0) > 220);
            total += 1;
        }
        advance += m.advance_width;
    }
    anyhow::ensure!(
        total > 0 && centers * 100 / total > 85 && gaps * 100 / total < 30,
        "static row cells not separated: centers={centers} gaps={gaps} total={total}"
    );
    println!("dot row pixels: centers={centers} bright_gaps={gaps} total={total}");
    Ok(())
}
