//! Hidden synthetic CPU edit/draw timing; not GPU completion or input-to-present.
use std::time::Instant;

use ctx_bar::config::Config;
use ctx_bar::model::ViewMode;
use ctx_bar_design::BarDesign;
use gpui_kit::{AppContext as _, EntityInputHandler as _};

use super::{app, host, settle, theme};

pub(super) async fn run(
    directory: &std::path::Path,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<()> {
    cx.update(|cx| theme::init(cx, "Lumen Dark"))?;
    let mut cases = Vec::new();
    for design in [BarDesign::DotMatrix, BarDesign::Unframed] {
        for view in [ViewMode::List, ViewMode::Tree] {
            let config = Config {
                design,
                view,
                reduce_motion: true,
                ..Default::default()
            };
            let mut bar = None;
            let handle = cx.open_window(host::options(&config, true), |window, cx| {
                let entity = cx.new(|cx| app::Bar::new(config, true, window, cx));
                bar = Some(entity.clone());
                cx.new(|cx| host::root(entity, window, cx))
            })?;
            let bar = bar.ok_or_else(|| anyhow::anyhow!("view unavailable"))?;
            let input = bar.read_with(cx, |bar, _| bar.review_input().unwrap());
            let mut samples = Vec::new();
            for index in 0..110 {
                let ms = cx.update_window(handle.into(), |_, window, cx| {
                    let start = Instant::now();
                    input.update(cx, |input, cx| {
                        input.focus(window, cx);
                        input.replace_text_in_range(
                            Some(0..input.value().len()),
                            if index % 2 == 0 {
                                "adjust appearance a"
                            } else {
                                "adjust appearance b"
                            },
                            window,
                            cx,
                        );
                    });
                    window.refresh();
                    window.draw(cx).clear(cx);
                    start.elapsed().as_secs_f64() * 1000.0
                })?;
                // Drain notifications outside the sample; this is not event-loop latency.
                settle(handle, cx).await?;
                if index >= 10 {
                    samples.push(ms);
                }
            }
            samples.sort_by(f64::total_cmp);
            let snapshot = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
            anyhow::ensure!(
                snapshot["ids"].as_array().unwrap().len()
                    == if view == ViewMode::Tree { 5 } else { 3 }
                    && snapshot["timer"] == false,
                "timing did not exercise its representative ready scope: {snapshot}"
            );
            cases.push(serde_json::json!({"design":design.id(),"view":view,"visible_choices":snapshot["ids"].as_array().unwrap().len(),"samples":samples.len(),"median_ms":samples[samples.len()/2],"p95_ms":samples[samples.len()*95/100],"snapshot":snapshot}));
            cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
        }
    }
    std::fs::write(
        directory.join("timing.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"scope":"hidden synthetic CPU input replacement plus GPUI draw; excludes GPU completion, compositor, physical input and inference", "cases":cases}),
        )?,
    )?;
    Ok(())
}
