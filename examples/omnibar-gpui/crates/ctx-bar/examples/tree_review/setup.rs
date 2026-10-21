//! In-app setup proof: real keystrokes, a loopback mock decisions endpoint, a
//! rejected key, then a good one. Scratch credentials path; never ~/.config.
use std::io::{Read as _, Write as _};

use ctx_bar::config::Config;
use ctx_bar::model::TreeVariant;
use ctx_bar_design::BarDesign;
use gpui_kit::AppContext as _;

use super::{Scene, app, host, key, save, settle, theme};

const KEY: &str = "sk-review-not-a-real-key";

/// One mock decisions endpoint that answers once per status, in order, and
/// returns what the client sent each time.
fn mock(statuses: &'static [&'static str]) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let catalog = ctx_bar::model::embedded_catalog().expect("catalog");
    let request = ctx_bar::model::Request {
        query: "switch to dark mode".into(),
        platform: ctx_bar::model::Platform::Macos,
        fixture: ctx_bar::model::Fixture::Desktop,
        presentation: ctx_bar::model::Presentation::Flat,
        node: "root".into(),
    };
    let offered = catalog.candidates(&request).expect("offered");
    let mut probabilities: serde_json::Map<String, serde_json::Value> = offered
        .iter()
        .map(|item| {
            (
                item.id.clone(),
                serde_json::json!(0.1 / offered.len() as f64),
            )
        })
        .collect();
    probabilities.insert("appearance.dark".into(), serde_json::json!(0.9));
    probabilities.insert("REFUSE".into(), serde_json::json!(0.0));
    let body = serde_json::json!({"answers": {"route": {"choice": "appearance.dark", "probabilities": probabilities}}}).to_string();
    listener.set_nonblocking(true).expect("nonblocking");
    let thread = std::thread::spawn(move || {
        statuses
            .iter()
            .map(|status| serve(&listener, status, &body))
            .collect()
    });
    (url, thread)
}

fn serve(listener: &std::net::TcpListener, status: &str, body: &str) -> String {
    // Never hang the harness: give up after 8s and report no connection.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Err(_) => return "NO CONNECTION".to_string(),
        }
    };
    socket.set_nonblocking(false).ok();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .ok();
    let mut seen = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = socket.read(&mut buf).unwrap_or(0);
        seen.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&seen).to_string();
        if let Some(head) = text.find("\r\n\r\n") {
            let length = text[..head]
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if seen.len() >= head + 4 + length {
                break;
            }
        }
        if n == 0 {
            break;
        }
    }
    let reply = if status.starts_with("200") {
        body
    } else {
        "{}"
    };
    let _ = write!(
        socket,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    String::from_utf8_lossy(&seen).to_string()
}

pub(super) async fn capture(
    directory: &std::path::Path,
    cx: &mut gpui_kit::AsyncApp,
) -> anyhow::Result<Vec<serde_json::Value>> {
    cx.update(|cx| theme::init(cx, "Lumen Dark"))?;
    let config = Config {
        design: BarDesign::DotMatrix,
        ..Default::default()
    };
    let mut bar = None;
    let handle = cx.open_window(host::options(&config, true), |window, cx| {
        let view = cx.new(|cx| app::Bar::new(config, true, window, cx));
        bar = Some(view.clone());
        cx.new(|cx| host::root(view, window, cx))
    })?;
    let bar = bar.ok_or_else(|| anyhow::anyhow!("view unavailable"))?;
    let scratch = std::env::temp_dir().join(format!("ctx-setup-review-{}", std::process::id()));
    let path = scratch.join("credentials.toml");
    bar.update(cx, |bar, _| bar.review_credentials_path(path.clone()));
    let input = bar.read_with(cx, |bar, _| bar.review_input().expect("input"));
    let snapshot =
        |cx: &mut gpui_kit::AsyncApp| bar.read_with(cx, |bar, cx| bar.review_snapshot(cx));
    let fill = |text: &str, cx: &mut gpui_kit::AsyncApp| {
        handle.update(cx, |_, window, cx| {
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.replace_all(text.to_string(), window, cx);
            })
        })
    };
    let mut captures = vec![];

    // "setup" is a command word: real keystrokes, then Enter opens the flow.
    for ch in "setup".chars() {
        key(handle, &ch.to_string(), cx)?;
    }
    settle(handle, cx).await?;
    anyhow::ensure!(
        snapshot(cx)["local_choices"] == 1 && snapshot(cx)["routable"] == false,
        "setup is not a local command"
    );
    key(handle, "enter", cx)?;
    settle(handle, cx).await?;
    let start = snapshot(cx);
    anyhow::ensure!(
        start["setup_step"] == "Endpoint" && start["setup_masked"] == false,
        "setup did not start: {start}"
    );

    // Same endpoint twice: first the key is rejected, then a good one works.
    let (url, server) = mock(&["401 Unauthorized", "200 OK"]);
    for expect_ok in [false, true] {
        if snapshot(cx)["setup_step"] == "Endpoint" {
            fill(&url, cx)?;
            key(handle, "enter", cx)?;
            settle(handle, cx).await?;
        }
        anyhow::ensure!(snapshot(cx)["setup_step"] == "Key", "endpoint not accepted");
        fill(KEY, cx)?;
        settle(handle, cx).await?;
        let masked = save(
            directory,
            Scene {
                scheme: "Lumen Dark",
                width: 680.0,
                design: BarDesign::DotMatrix,
                variant: TreeVariant::Faint,
                name: "setup-key",
            },
            handle,
            &bar,
            cx,
        )?;
        anyhow::ensure!(masked["setup_masked"] == true, "key field is not masked");
        anyhow::ensure!(
            masked["input_diagnostics"]["grid_passes"] == 0,
            "masked key was painted by the dot grid"
        );
        key(handle, "enter", cx)?;
        settle(handle, cx).await?;
        anyhow::ensure!(
            snapshot(cx)["setup_step"] == "Token",
            "key step did not advance"
        );
        key(handle, "enter", cx)?; // skip the optional listener token
        let mut done = snapshot(cx);
        for _ in 0..60 {
            if done["setup_step"] != "Testing" {
                break;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            settle(handle, cx).await?;
            done = snapshot(cx);
        }
        anyhow::ensure!(
            !done.to_string().contains(KEY),
            "the key leaked into the UI snapshot"
        );
        if expect_ok {
            anyhow::ensure!(
                done["setup_step"].is_null()
                    && done["direct"].as_str().is_some_and(|d| d.starts_with(&url)),
                "good key not saved: {done}"
            );
            anyhow::ensure!(
                done["status_text"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Jev connected")),
                "no success status: {done}"
            );
            let text = std::fs::read_to_string(&path)?;
            anyhow::ensure!(
                text.contains(&url) && text.contains(KEY),
                "credentials file incomplete"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                anyhow::ensure!(
                    std::fs::metadata(&path)?.permissions().mode() & 0o777 == 0o600,
                    "credentials not owner-only"
                );
            }
        } else {
            anyhow::ensure!(
                done["setup_step"] == "Key",
                "rejected key did not return to the key step: {done}"
            );
            anyhow::ensure!(
                done["status_text"]
                    .as_str()
                    .is_some_and(|s| s.contains("rejected the key")),
                "no rejection message: {done}"
            );
            anyhow::ensure!(!path.exists(), "a rejected key was saved");
        }
        captures.push(done);
    }
    let sent = server
        .join()
        .map_err(|_| anyhow::anyhow!("mock panicked"))?;
    anyhow::ensure!(
        sent.len() == 2
            && sent
                .iter()
                .all(|request| request.contains(&format!("Bearer {KEY}"))
                    && request.contains("switch to dark mode")),
        "probes did not send the key and synthetic query: {:?}",
        sent.iter()
            .map(|request| &request[..request.len().min(40)])
            .collect::<Vec<_>>()
    );
    // Esc during a new setup cancels without touching saved credentials.
    fill("setup", cx)?;
    settle(handle, cx).await?;
    key(handle, "enter", cx)?;
    key(handle, "escape", cx)?;
    settle(handle, cx).await?;
    anyhow::ensure!(
        snapshot(cx)["setup_step"].is_null() && path.exists(),
        "Esc did not cancel cleanly"
    );
    std::fs::remove_dir_all(&scratch)?;
    cx.update_window(handle.into(), |_, window, _| window.remove_window())?;
    Ok(captures)
}
