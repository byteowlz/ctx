//! Explicit, synthetic native scene capture. Never captures the desktop/window
//! compositor, reads a descriptor or contacts Jev. Hidden scenes are not proof of
//! OS blur, on-screen focus, real IME, dictation or accessibility acceptance.
use std::path::PathBuf;
#[cfg(feature = "native-review")]
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
#[cfg(feature = "native-review")]
use ctx_bar_design::BarDesign;
#[cfg(feature = "native-review")]
use gpui_kit::AppContext as _;
use gpui_kit::component::Root;
use gpui_kit::{AsyncApp, Entity, WindowHandle};

use crate::app::Bar;

pub fn extract_args(args: Vec<String>) -> Result<(Vec<String>, Option<PathBuf>)> {
    let mut ordinary = Vec::new();
    let mut directory = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != "--review-dir" {
            ordinary.push(arg);
            continue;
        }
        if directory.is_some() {
            bail!("--review-dir may only be specified once");
        }
        let path = PathBuf::from(
            args.next()
                .context("--review-dir requires an absolute path")?,
        );
        if !path.is_absolute() {
            bail!("--review-dir must be an absolute path");
        }
        directory = Some(path);
    }
    Ok((ordinary, directory))
}

#[cfg(not(feature = "native-review"))]
pub async fn capture(
    _window: WindowHandle<Root>,
    _bar: Entity<Bar>,
    _directory: PathBuf,
    _cx: &mut AsyncApp,
) -> Result<()> {
    bail!("Scene capture requires --features native-review; ordinary startup has no capture API")
}

#[cfg(feature = "native-review")]
pub async fn capture(
    window: WindowHandle<Root>,
    bar: Entity<Bar>,
    directory: PathBuf,
    cx: &mut AsyncApp,
) -> Result<()> {
    std::fs::create_dir_all(&directory)?;
    let mut captures = Vec::new();
    for design in BarDesign::ALL {
        for (state, query) in [("empty", ""), ("suggestions", "light mode")] {
            window.update(cx, |_, window, cx| {
                bar.update(cx, |bar, cx| bar.review_scene(design, query, window, cx));
            })?;
            // Hidden windows do not request normal on-screen frames. Draw first
            // to initiate progressive resize, then let native bounds settle.
            cx.update_window(window.into(), |_, window, cx| {
                let _ = window.draw(cx);
            })?;
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let path = directory.join(format!("{}-{state}.png", design.id()));
            // Untyped window update does not lease Root while draw renders it.
            let dimensions = cx.update_window(window.into(), |_, window, cx| -> Result<_> {
                let _ = window.draw(cx);
                let image = window.render_to_image()?;
                let dimensions = (image.width(), image.height());
                image.save(&path)?;
                Ok(dimensions)
            })??;
            captures.push(serde_json::json!({"file":path.file_name().and_then(|p|p.to_str()), "design":design.id(), "state":state, "pixels":dimensions}));
        }
    }
    window.update(cx, |_, window, cx| {
        bar.update(cx, |bar, cx| {
            bar.review_scene(BarDesign::DotMatrix, "ctx theme", window, cx)
        });
    })?;
    cx.update_window(window.into(), |_, window, cx| {
        let _ = window.draw(cx);
    })?;
    cx.background_executor()
        .timer(Duration::from_millis(60))
        .await;
    cx.update_window(window.into(), |_, window, cx| -> Result<()> {
        let _ = window.draw(cx);
        window
            .render_to_image()?
            .save(directory.join("local-theme-menu.png"))?;
        Ok(())
    })??;
    let mut switching = Vec::new();
    for design in BarDesign::ALL {
        window.update(cx, |_, window, cx| {
            bar.update(cx, |bar, cx| {
                bar.review_scene(
                    BarDesign::DotMatrix,
                    &format!("ctx theme {}", design.id()),
                    window,
                    cx,
                )
            });
        })?;
        cx.background_executor()
            .timer(Duration::from_millis(60))
            .await;
        let before = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
        if before["routable"] != false || before["local_choices"] != 1 {
            bail!(
                "Local command routing/list invariant failed for {}",
                design.id()
            );
        }
        cx.update_window(window.into(), |_, window, cx| -> Result<()> {
            let _ = window.draw(cx);
            window.dispatch_keystroke(gpui_kit::Keystroke::parse("enter")?, cx);
            Ok(())
        })??;
        cx.background_executor()
            .timer(Duration::from_millis(60))
            .await;
        let after = bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
        if after["design"] != design.id() || after["input_empty"] != true || after["timer"] != false
        {
            bail!(
                "Native Enter did not apply/clear local design {}: {after}",
                design.id()
            );
        }
        switching.push(serde_json::json!({"command":format!("ctx theme {}", design.id()),"before":before,"after":after,"native_enter":true}));
    }
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind":"native-gpui-hidden-scene-render",
            "synthetic":true,"model_requests":false,"desktop_capture":false,
            "captures":captures,"local_menu":"local-theme-menu.png","switching":switching,
            "limits":["No OS compositor/blur proof","No on-screen focus/keyboard/IME/dictation acceptance","Programmatic input fixtures, not real user transcripts"]
        }))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_is_explicit_absolute_and_removed_from_config_flags() {
        let (args, review) = extract_args(vec![
            "--design".into(),
            "lens".into(),
            "--review-dir".into(),
            "/tmp/synthetic-review".into(),
        ])
        .unwrap();
        assert_eq!(args, ["--design", "lens"]);
        assert_eq!(review, Some(PathBuf::from("/tmp/synthetic-review")));
        assert!(extract_args(vec!["--review-dir".into()]).is_err());
        assert!(extract_args(vec!["--review-dir".into(), "relative".into()]).is_err());
        assert!(
            extract_args(vec![
                "--review-dir".into(),
                "/tmp/a".into(),
                "--review-dir".into(),
                "/tmp/b".into()
            ])
            .is_err()
        );
        assert!(extract_args(vec![]).unwrap().1.is_none());
    }
}
