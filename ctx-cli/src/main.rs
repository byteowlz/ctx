use clap::{Parser, ValueEnum};

use ctx_core::capture::{CaptureEnvelope, DisplayInfo, WindowInfo};
use ctx_core::config;
use ctx_core::platform::{CaptureRequest, ContextProvider, DesktopPlatform, NoopPlatform};

#[derive(Parser, Debug)]
#[command(name = "ctx")]
#[command(about = "Context capture CLI (scaffold)", long_about = None)]
struct Cli {
    /// Emit capture output as JSON
    #[arg(long)]
    json: bool,
    /// Save capture JSON to the configured capture directory
    #[arg(long)]
    save: bool,
    /// Platform provider to use (desktop interacts with clipboard/screenshots)
    #[arg(long, value_enum, default_value_t = Provider::Desktop)]
    provider: Provider,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Provider {
    Desktop,
    Noop,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let cfg = config::load(config::default_app_name())?;
    let request = CaptureRequest::from_config(&cfg.capture, cfg.output.capture_dir.clone());

    let platform: Box<dyn ContextProvider> = match cli.provider {
        Provider::Desktop => Box::new(DesktopPlatform::default()),
        Provider::Noop => Box::new(NoopPlatform::default()),
    };
    let result = platform.capture(&request)?;
    let envelope = CaptureEnvelope::new(result);
    let saved_path = if cli.save {
        Some(save_capture(&envelope, &cfg.output.capture_dir)?)
    } else {
        None
    };

    if cli.json {
        print_json(&envelope)?;
    } else {
        print_summary(&envelope);
    }

    if let Some(path) = saved_path {
        if cli.json {
            eprintln!("Saved capture to {}", path.display());
        } else {
            println!("Saved capture to {}", path.display());
        }
    }

    Ok(())
}

fn print_json(envelope: &CaptureEnvelope) -> anyhow::Result<()> {
    let output = serde_json::to_string_pretty(envelope)?;
    println!("{output}");
    Ok(())
}

fn print_summary(envelope: &CaptureEnvelope) {
    println!("Capture summary");
    println!("Session: {}", envelope.metadata.session_id);
    if let Ok(ts) = envelope
        .metadata
        .captured_at
        .format(&time::format_description::well_known::Rfc3339)
    {
        println!("Captured at: {ts}");
    }

    let result = &envelope.context;

    println!("Platform: {}", result.system.platform);
    if let Some(os_version) = &result.system.os_version {
        println!("OS: {os_version}");
    }
    if let Some(kernel_version) = &result.system.kernel_version {
        println!("Kernel: {kernel_version}");
    }
    if let Some(hostname) = &result.system.hostname {
        println!("Hostname: {hostname}");
    }

    print_displays(&result.displays);
    print_windows(&result.windows);

    println!(
        "Clipboard: {}",
        if result.clipboard.enabled {
            "enabled (stubbed)"
        } else {
            "disabled"
        }
    );
    if let Some(note) = &result.clipboard.note {
        println!("  Note: {note}");
    }

    println!(
        "Screenshots: {}",
        if result.screenshots.enabled {
            format!("{} capture(s) (stubbed)", result.screenshots.captures.len())
        } else {
            "disabled".to_string()
        }
    );
    for capture in &result.screenshots.captures {
        println!("  Mode: {:?}", capture.mode);
        if let Some(path) = &capture.path {
            println!("  Path: {}", path.display());
        }
        if let Some(note) = &capture.note {
            println!("  Note: {note}");
        }
    }

    println!(
        "Accessibility: {} (depth {})",
        if result.accessibility.enabled {
            "enabled (stubbed)"
        } else {
            "disabled"
        },
        result.accessibility.depth
    );
    if let Some(note) = &result.accessibility.note {
        println!("  Note: {note}");
    }

    if !result.notes.is_empty() {
        println!("Notes:");
        for note in &result.notes {
            println!("- {note}");
        }
    }
}

fn save_capture(
    envelope: &CaptureEnvelope,
    capture_dir: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(capture_dir)?;

    let timestamp = envelope
        .metadata
        .captured_at
        .format(&time::format_description::well_known::Rfc3339)?
        .replace(':', "-");
    let file_name = format!("capture-{}-{timestamp}.json", envelope.metadata.session_id);
    let path = capture_dir.join(file_name);

    let payload = serde_json::to_vec_pretty(envelope)?;
    std::fs::write(&path, payload)?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn save_capture_writes_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path();

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
            std::env::set_var("XDG_DATA_HOME", base.join("data"));
            std::env::set_var("XDG_STATE_HOME", base.join("state"));
        }

        let cfg = config::load(config::default_app_name()).expect("load config");
        let request = CaptureRequest::from_config(&cfg.capture, cfg.output.capture_dir.clone());

        let platform = NoopPlatform::default();
        let result = platform.capture(&request).expect("capture");
        let envelope = CaptureEnvelope::new(result);

        let saved = save_capture(&envelope, &cfg.output.capture_dir).expect("save");
        let contents = fs::read_to_string(&saved).expect("read saved");
        let parsed: serde_json::Value = serde_json::from_str(&contents).expect("json");
        assert!(parsed.get("metadata").is_some());
        assert!(parsed.get("context").is_some());

        let saved_envelope: CaptureEnvelope =
            serde_json::from_str(&contents).expect("envelope roundtrip");
        assert!(!saved_envelope.metadata.session_id.is_empty());

        let file_name = saved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(file_name.contains(&saved_envelope.metadata.session_id));
        assert!(saved.starts_with(&cfg.output.capture_dir));
    }
}

fn print_displays(displays: &[DisplayInfo]) {
    if displays.is_empty() {
        println!("Displays: none reported (stubbed)");
        return;
    }

    println!("Displays:");
    for display in displays {
        println!(
            "- #{index} {width}x{height}{scale}",
            index = display.index,
            width = display.width,
            height = display.height,
            scale = display
                .scale_factor
                .map(|s| format!("@{s}x"))
                .unwrap_or_default()
        );
        if let Some(name) = &display.name {
            println!("  Name: {name}");
        }
    }
}

fn print_windows(windows: &[WindowInfo]) {
    if windows.is_empty() {
        println!("Windows: none reported (stubbed)");
        return;
    }

    println!("Windows:");
    for window in windows {
        println!(
            "- {title} [{app}] {focused}",
            title = window.title.as_deref().unwrap_or("<untitled>"),
            app = window.app_name.as_deref().unwrap_or("<unknown app>"),
            focused = if window.focused { "(focused)" } else { "" }
        );
        if let Some(bounds) = &window.bounds {
            println!(
                "  Bounds: x={}, y={}, w={}, h={}",
                bounds.x, bounds.y, bounds.width, bounds.height
            );
        }
    }
}
