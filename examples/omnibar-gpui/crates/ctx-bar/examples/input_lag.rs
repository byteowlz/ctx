//! Run: cargo run -p ctx-bar --features native-review --example input_lag
//! Add -- --assert-single-grid for the real-paint submission regression.
//! --assert-expanded-grid writes synthetic scene PNGs to /tmp/ctx-grid-regression.
//! --local-menu benchmarks native replacements between ctx and ctx theme.
//! Hidden native windows, fixed synthetic fixtures only; no desktop capture/model.
#[cfg(feature = "native-review")]
#[path = "../src/app.rs"]
#[allow(dead_code)] // Production module included by this bounded diagnostic harness.
mod app;
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
    anyhow::bail!("input_lag requires --features native-review")
}

#[cfg(feature = "native-review")]
fn main() {
    use ctx_bar::config::Config;
    use ctx_bar_design::BarDesign;
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext as _, EntityInputHandler as _, WindowBackgroundAppearance, WindowBounds,
        WindowKind, WindowOptions, px, size,
    };
    use std::time::{Duration, Instant};

    let args: Vec<_> = std::env::args().collect();
    let assert_budget = args.iter().any(|arg| arg == "--assert-single-grid");
    let expect_plain = args.iter().any(|arg| arg == "--expect-plain");
    let append_only = args.iter().any(|arg| arg == "--append-only");
    let local_menu = args.iter().any(|arg| arg == "--local-menu");
    let width = args
        .windows(2)
        .find(|pair| pair[0] == "--width")
        .map(|pair| pair[1].parse::<f32>().expect("numeric synthetic width"))
        .unwrap_or(680.0);
    gpui_kit::application().with_assets(gpui_kit::assets::AllAssets).run(move |cx| {
        gpui_kit::init(cx);
        if let Err(error) = presentation::register_font(cx) {
            eprintln!("input-lag registration: {error}");
            std::process::exit(1);
        }
        cx.spawn(async move |cx| {
            let result: anyhow::Result<()> = async {
                if args.iter().any(|arg| arg == "--assert-expanded-grid") {
                    return expanded_grid(width, cx).await;
                }
                for design in [BarDesign::Unframed, BarDesign::DotMatrix] {
                    let config = Config { design, width, ..Default::default() };
                    let height = ctx_bar::layout::layout(design, 0, false, config.height).height;
                    let options = WindowOptions {
                        kind: WindowKind::PopUp, titlebar: None, show: false, focus: false,
                        window_background: WindowBackgroundAppearance::Transparent,
                        window_bounds: Some(cx.update(|cx| WindowBounds::centered(size(px(config.width), px(height)), cx))),
                        ..Default::default()
                    };
                    let mut bar = None;
                    let handle = cx.open_window(options, |window, cx| {
                        let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
                        bar = Some(view.clone());
                        cx.new(|cx| Root::new(view, window, cx).bordered(false))
                    })?;
                    let bar = bar.unwrap();
                    let input = bar.read_with(cx, |bar, _| bar.review_input().unwrap());
                    if local_menu {
                        handle.update(cx, |_, window, cx| input.update(cx, |input, cx| input.replace_text_in_range(None, "ctx", window, cx)))?;
                    }
                    for _ in 0..3 {
                        cx.update_window(handle.into(), |_, window, cx| { window.draw(cx).clear(cx); })?;
                        cx.background_executor().timer(Duration::from_millis(20)).await;
                    }
                    cx.update(|cx| cx.set_global(presentation::InputDiagnostics::default()));
                    let mut samples = Vec::new();
                    for edit in 0..200 {
                        let started = Instant::now();
                        handle.update(cx, |_, window, cx| {
                            input.update(cx, |input, cx| {
                                benchmark_edit(input, edit, (append_only, local_menu), window, cx);
                            });
                        })?;
                        cx.update_window(handle.into(), |_, window, cx| { window.draw(cx).clear(cx); })?;
                        samples.push(started.elapsed().as_micros());
                        let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
                        let expected = if append_only {
                            edit + 1
                        } else if edit % 40 < 20 { edit % 40 + 1 } else { 39 - edit % 40 };
                        let expected_query = if local_menu { if edit.is_multiple_of(2) { "ctx".to_owned() } else { "ctx theme".to_owned() } } else { "a".repeat(expected) };
                        anyhow::ensure!(snapshot["query"] == expected_query, "editing/state diverged: {snapshot}");
                        if local_menu {
                            anyhow::ensure!(snapshot["local_choices"] == 10, "benchmark lost local menu");
                        }
                    }
                    samples.sort_unstable();
                    let stats = cx.update(|cx| cx.global::<presentation::InputDiagnostics>().clone());
                    println!("[ctx-input-lag] {}", serde_json::json!({
                        "design":design.id(), "workload":if local_menu { "local-menu-200" } else if append_only { "append-200" } else { "insert-delete-200" }, "edits":200, "median_us":samples[100], "p95_us":samples[190], "max_us":samples[199], "stats":stats,
                        "limits":"hidden-native CPU edit+draw, not physical keyboard/compositor/IME acceptance"
                    }));
                    let before_idle = stats.observations;
                    for _ in 0..10 {
                        cx.update_window(handle.into(), |_, window, cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                    }
                    let after_idle = cx.update(|cx| cx.global::<presentation::InputDiagnostics>().observations);
                    println!("[ctx-input-lag] idle design={} draws=10 observations_delta={}", design.id(), after_idle - before_idle);
                    anyhow::ensure!(before_idle == after_idle, "idle frame -> input notify feedback loop");
                    // Real app/Input/canvas seam. Detect duplicate grid even though
                    // ordinary correctness tests and successful captures pass.
                    if assert_budget && design == BarDesign::DotMatrix {
                        if expect_plain {
                            anyhow::ensure!(stats.mask_successes == 0 && stats.submitted_quads == 0, "oversized stage did not leave plain visible input");
                        } else {
                            anyhow::ensure!(stats.mask_successes == stats.mask_attempts && stats.mask_successes >= 200, "workload did not reach successful native masks");
                        }
                        anyhow::ensure!(stats.grid_passes == stats.mask_successes, "duplicate grid submitted: {} passes for {} successful masks", stats.grid_passes, stats.mask_successes);
                    }
                    if design == BarDesign::DotMatrix {
                        handle.update(cx, |_, window, cx| {
                            input.update(cx, |input, cx| {
                                input.set_value("abc", window, cx);
                                input.set_selected_range(0..2, cx);
                            });
                        })?;
                        cx.update_window(handle.into(), |_, window, cx| { window.draw(cx).clear(cx); })?;
                        anyhow::ensure!(input.read_with(cx, |input, _| input.selected_range()) == (0..2), "presentation mutated native selection");
                        handle.update(cx, |_, window, cx| input.update(cx, |input, cx| input.replace_text_in_range(None, "a", window, cx)))?;
                        cx.update_window(handle.into(), |_, window, cx| { window.draw(cx).clear(cx); })?;
                        anyhow::ensure!(input.read_with(cx, |input, _| input.value()) == "ac", "native selection replacement failed");
                        println!("[ctx-input-lag] safety selection_replacement=passed");
                        // Safety cases deliberately go through the same native paint seam.
                        for (case, text) in [("unicode", "é"), ("ascii-composition", "abc")] {
                            cx.update(|cx| cx.set_global(presentation::InputDiagnostics::default()));
                            handle.update(cx, |_, window, cx| {
                                input.update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                    if case == "ascii-composition" {
                                        input.replace_and_mark_text_in_range(None, text, Some(0..text.len()), window, cx);
                                    } else {
                                        input.replace_text_in_range(None, text, window, cx);
                                    }
                                });
                            })?;
                            cx.update_window(handle.into(), |_, window, cx| { window.draw(cx).clear(cx); })?;
                            let stats = cx.update(|cx| cx.global::<presentation::InputDiagnostics>().clone());
                            anyhow::ensure!(stats.mask_attempts > 0 && stats.mask_successes == 0 && stats.submitted_quads == 0, "{case} must keep visible native ink: {}", serde_json::to_string(&stats)?);
                            anyhow::ensure!(input.read_with(cx, |input, _| input.value()) == text, "fallback lost native value");
                            println!("[ctx-input-lag] safety case={case} native_value_preserved=true grid_quads=0");
                            handle.update(cx, |_, window, cx| input.update(cx, |input, cx| input.unmark_text(window, cx)))?;
                        }
                    }
                    cx.update_window(handle.into(), |_, window, _cx| window.remove_window())?;
                }
                Ok(())
            }.await;
            if let Err(error) = result {
                eprintln!("[ctx-input-lag] FAILED: {error}");
                std::process::exit(1);
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
}

#[cfg(feature = "native-review")]
fn benchmark_edit(
    input: &mut gpui_kit::component::input::InputState,
    edit: usize,
    (append_only, local_menu): (bool, bool),
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<gpui_kit::component::input::InputState>,
) {
    use gpui_kit::EntityInputHandler as _;
    if local_menu {
        let end = input.value().len();
        input.replace_text_in_range(
            Some(0..end),
            if edit.is_multiple_of(2) {
                "ctx"
            } else {
                "ctx theme"
            },
            window,
            cx,
        );
    } else if !append_only && edit % 40 >= 20 {
        let end = input.value().len(); // Synthetic ASCII, UTF-16 == bytes.
        input.replace_text_in_range(Some(end.saturating_sub(1)..end), "", window, cx);
    } else {
        input.replace_text_in_range(None, "a", window, cx);
    }
}

/// Exercise progressive disclosure through the production Bar/Input/paint seam.
/// Images contain only fixed synthetic review data, never a desktop capture.
#[cfg(feature = "native-review")]
async fn expanded_grid(width: f32, cx: &mut gpui_kit::AsyncApp) -> anyhow::Result<()> {
    use ctx_bar::config::Config;
    use ctx_bar_design::BarDesign;
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext as _, EntityInputHandler as _, WindowBounds, WindowKind, WindowOptions, px, size,
    };
    use std::time::Duration;

    let config = Config {
        design: BarDesign::DotMatrix,
        width,
        ..Default::default()
    };
    let options = WindowOptions {
        kind: WindowKind::PopUp,
        titlebar: None,
        show: false,
        focus: false,
        window_background: gpui_kit::WindowBackgroundAppearance::Transparent,
        window_bounds: Some(cx.update(|cx| WindowBounds::centered(size(px(width), px(63.0)), cx))),
        ..Default::default()
    };
    let mut bar = None;
    let handle = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
        bar = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx).bordered(false))
    })?;
    let bar = bar.unwrap();
    let input = bar.read_with(cx, |bar, _| bar.review_input().unwrap());
    for scheme in ["Lumen Dark", "Lumen Light"] {
        cx.update(|cx| theme::init(cx, scheme))?;
        let directory = format!("/tmp/ctx-grid-regression/{width}/{scheme}");
        std::fs::create_dir_all(&directory)?;
        for (case, query, composing) in [
            ("ctx", "ctx", false),
            ("ctx-theme", "ctx theme", false),
            ("collapsed", "", false),
            ("ctx-again", "ctx", false),
            ("idle", "", false),
            ("unicode", "é", false),
            ("composition", "abc", true),
        ] {
            handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| {
                    input.unmark_text(window, cx);
                    // Native dispatch ranges use UTF-16, including the fallback fixture.
                    let end = input.value().encode_utf16().count();
                    if composing {
                        input.replace_and_mark_text_in_range(
                            Some(0..end),
                            query,
                            Some(0..query.len()),
                            window,
                            cx,
                        );
                    } else {
                        input.replace_text_in_range(Some(0..end), query, window, cx);
                    }
                });
            })?;
            cx.update_window(handle.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
            })?;
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            cx.update(|cx| cx.set_global(presentation::InputDiagnostics::default()));
            let (dimensions, center, gap, white_pixels, white_cells, bright_gaps) = cx
                .update_window(handle.into(), |_, window, cx| -> anyhow::Result<_> {
                    window.refresh();
                    window.draw(cx).clear(cx);
                    let image = window.render_to_image()?;
                    image.save(format!("{directory}/{case}.png"))?;
                    let scale = image.width() as f32 / width;
                    let sample =
                        |x: f32, y: f32| image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
                    let white_pixels = image
                        .enumerate_pixels()
                        .filter(|(_, y, p)| {
                            *y < (63.0 * scale) as u32
                                && p.0[0] > 220
                                && p.0[1] > 220
                                && p.0[2] > 220
                        })
                        .count();
                    let mut white_cells = 0;
                    let mut bright_gaps = 0;
                    for row in 0..21 {
                        for column in 0..(width / 3.0).floor() as u32 {
                            let x = column as f32 * 3.0;
                            let y = row as f32 * 3.0;
                            white_cells += usize::from(sample(x + 1.5, y + 1.5)[0] > 220);
                            bright_gaps += usize::from(sample(x, y + 1.5)[0] > 3);
                            bright_gaps += usize::from(sample(x + 1.5, y)[0] > 3);
                        }
                    }
                    Ok((
                        (image.width(), image.height()),
                        sample(((width - 80.0) / 3.0).floor() * 3.0 + 1.5, 31.5),
                        sample(((width - 80.0) / 3.0).floor() * 3.0 + 3.0, 31.5),
                        white_pixels,
                        white_cells,
                        bright_gaps,
                    ))
                })??;
            let stats = cx.update(|cx| cx.global::<presentation::InputDiagnostics>().clone());
            let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            println!(
                "[ctx-expanded-grid] {}",
                serde_json::json!({"scheme":scheme, "case":case, "pixels":dimensions, "cell_center":center, "cell_gap":gap, "white_pixels":white_pixels, "white_cells":white_cells, "bright_gaps":bright_gaps, "stats":stats, "local_choices":snapshot["local_choices"]})
            );
            // Marked text is deliberately not a committed InputEvent::Change.
            if !composing {
                anyhow::ensure!(snapshot["query"] == query, "native review query diverged");
            }
            anyhow::ensure!(
                input.read_with(cx, |input, _| input.value()) == query,
                "native value diverged"
            );
            if query.starts_with("ctx") {
                anyhow::ensure!(
                    snapshot["local_choices"] == 10,
                    "expanded fixture did not reach local menu"
                );
            }
            anyhow::ensure!(
                stats.stage_bounds == [0.0, 0.0, width, BarDesign::DotMatrix.bar_height()],
                "{case}: grid stage moved or grew with results"
            );
            if composing
                || !query.is_ascii()
                || matrix::Mask::new(width, BarDesign::DotMatrix.bar_height()).is_none()
            {
                anyhow::ensure!(
                    stats.mask_attempts > 0
                        && stats.mask_successes == 0
                        && stats.grid_passes == 0
                        && stats.submitted_quads == 0
                        && center[0] == 0
                        && white_pixels > 100
                        && bright_gaps > 0,
                    "{case}: expected visible ordinary native ink, not grid"
                );
                continue;
            }
            anyhow::ensure!(
                stats.mask_successes > 0
                    && stats.mask_successes == stats.mask_attempts
                    && stats.grid_passes == stats.mask_successes,
                "{case}: expected one successful native grid: {}",
                serde_json::to_string(&stats)?
            );
            anyhow::ensure!(
                center[0] >= 12
                    && center[0] <= 20
                    && gap[0] <= 3
                    && white_pixels > 100
                    && white_cells > 20
                    && bright_gaps == 0,
                "{case}: scene pixels lack separated cells/native letter mask"
            );
        }
    }
    cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
    Ok(())
}
