mod bundle;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

use ctx_core::capture::{AppInfo, CaptureEnvelope, DisplayInfo, WindowInfo};
use ctx_core::config;
use ctx_core::current::{self, ContextKind, CurrentContextReport};
use ctx_core::ingest;
use ctx_core::platform::{CaptureRequest, ContextProvider, DesktopPlatform, NoopPlatform};

#[derive(Parser, Debug)]
#[command(name = "ctx")]
#[command(about = "Context capture CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    // --- Capture flags (used when no subcommand or `capture` subcommand) ---
    /// Emit capture output as JSON
    #[arg(long)]
    json: bool,
    /// Save capture JSON to the configured capture directory
    #[arg(long)]
    save: bool,
    /// Path to a config file to load (highest priority after CLI flags)
    #[arg(long)]
    config: Option<std::path::PathBuf>,
    /// Enable clipboard capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_clipboard")]
    clipboard: bool,
    /// Disable clipboard capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "clipboard")]
    no_clipboard: bool,
    /// Enable screenshot capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_screenshots")]
    screenshots: bool,
    /// Disable screenshot capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "screenshots")]
    no_screenshots: bool,
    /// Enable accessibility capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_accessibility")]
    accessibility: bool,
    /// Disable accessibility capture
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "accessibility")]
    no_accessibility: bool,
    /// Enable action layer
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_actions")]
    actions: bool,
    /// Disable action layer
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "actions")]
    no_actions: bool,
    /// Override accessibility depth
    #[arg(long)]
    accessibility_depth: Option<u8>,
    /// Override screenshot image quality (0-100)
    #[arg(long)]
    image_quality: Option<u8>,
    /// Override screenshot timeout in milliseconds
    #[arg(long)]
    screenshot_timeout_ms: Option<u64>,
    /// Override capture output directory
    #[arg(long)]
    capture_dir: Option<String>,
    /// Override state file path
    #[arg(long, global = true)]
    state_file: Option<String>,
    /// Platform provider to use (desktop interacts with clipboard/screenshots)
    #[arg(long, value_enum, default_value_t = Provider::Desktop)]
    provider: Provider,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture current context (default when no subcommand is given)
    Capture,
    /// Show or edit configuration
    Config {
        /// Interactively edit configuration
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,
        /// Show config file path only
        #[arg(short = 'p', long = "path")]
        path: bool,
    },
    /// Show or update the lightweight current-context state file
    Current {
        /// Emit current context as JSON
        #[arg(long)]
        json: bool,
        #[command(subcommand)]
        action: Option<CurrentAction>,
    },
    /// Manage context bundles for Agent Handoff
    Bundle(bundle::BundleCli),
    /// Watch screenshot sources (clipboard + screenshot dirs) and emit JSONL detection events
    Watch {
        /// Directory to watch (repeatable; defaults to platform screenshot dirs)
        #[arg(long = "dir")]
        dirs: Vec<std::path::PathBuf>,
        /// Disable clipboard image polling
        #[arg(long = "no-clipboard", id = "watch_no_clipboard")]
        no_clipboard: bool,
        /// Poll interval in milliseconds
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
        /// Only consider files modified within this many seconds (0 = no limit)
        #[arg(long, default_value_t = 600)]
        max_age_secs: u64,
        /// Run a single detection pass and exit
        #[arg(long)]
        once: bool,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Provider {
    Desktop,
    Noop,
}

#[derive(Debug, Subcommand)]
enum CurrentAction {
    /// Report the complete active context from an event source
    Report {
        /// Source name, e.g. shell, tmux, pi, aerospace
        #[arg(long)]
        source: Option<String>,
        /// Explicit context kind for stale-field prevention
        #[arg(long, value_enum)]
        kind: Option<CurrentKindArg>,
        /// Current application name
        #[arg(long)]
        app: Option<String>,
        /// Current application bundle ID when available
        #[arg(long)]
        bundle_id: Option<String>,
        /// Current window title
        #[arg(long)]
        window: Option<String>,
        /// Current workspace/space name
        #[arg(long)]
        workspace: Option<String>,
        /// Current URL, only when the reported context is actually a browser/web context
        #[arg(long)]
        url: Option<String>,
        /// Current working directory, only when the reported context is actually a terminal/shell context
        #[arg(long)]
        cwd: Option<String>,
        /// Current project or repository name/path
        #[arg(long)]
        project: Option<String>,
        /// Terminal nesting depth of the reporter (e.g. tmux=1, herdr inside tmux=2); deeper fresh reports win over shallower ones
        #[arg(long)]
        depth: Option<u32>,
        /// Emit current context as JSON after updating
        #[arg(long)]
        json: bool,
    },
    /// Deprecated alias for report
    Set {
        /// Source name, e.g. shell, tmux, pi, aerospace
        #[arg(long)]
        source: Option<String>,
        /// Explicit context kind for stale-field prevention
        #[arg(long, value_enum)]
        kind: Option<CurrentKindArg>,
        /// Current application name
        #[arg(long)]
        app: Option<String>,
        /// Current application bundle ID when available
        #[arg(long)]
        bundle_id: Option<String>,
        /// Current window title
        #[arg(long)]
        window: Option<String>,
        /// Current workspace/space name
        #[arg(long)]
        workspace: Option<String>,
        /// Current URL, only when the reported context is actually a browser/web context
        #[arg(long)]
        url: Option<String>,
        /// Current working directory, only when the reported context is actually a terminal/shell context
        #[arg(long)]
        cwd: Option<String>,
        /// Current project or repository name/path
        #[arg(long)]
        project: Option<String>,
        /// Terminal nesting depth of the reporter (e.g. tmux=1, herdr inside tmux=2); deeper fresh reports win over shallower ones
        #[arg(long)]
        depth: Option<u32>,
        /// Emit current context as JSON after updating
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CurrentKindArg {
    Application,
    Terminal,
    Browser,
    Editor,
    Unknown,
}

impl From<CurrentKindArg> for ContextKind {
    fn from(value: CurrentKindArg) -> Self {
        match value {
            CurrentKindArg::Application => Self::Application,
            CurrentKindArg::Terminal => Self::Terminal,
            CurrentKindArg::Browser => Self::Browser,
            CurrentKindArg::Editor => Self::Editor,
            CurrentKindArg::Unknown => Self::Unknown,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Config { interactive, path }) => {
            if *path {
                cmd_config_path()?;
            } else if *interactive {
                cmd_config_interactive()?;
            } else {
                cmd_config_show()?;
            }
        }
        Some(Command::Bundle(bundle_cli)) => {
            let cfg = config::load(config::default_app_name())?;
            bundle::run(bundle_cli, &cfg)?;
        }
        Some(Command::Current { json, action }) => {
            cmd_current(&cli, *json, action.as_ref())?;
        }
        Some(Command::Watch {
            dirs,
            no_clipboard,
            interval_ms,
            max_age_secs,
            once,
        }) => {
            cmd_watch(dirs, *no_clipboard, *interval_ms, *max_age_secs, *once)?;
        }
        Some(Command::Capture) | None => {
            cmd_capture(&cli)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Capture command
// ---------------------------------------------------------------------------

fn cmd_capture(cli: &Cli) -> anyhow::Result<()> {
    let mut overrides = config::ConfigOverrides::default();
    if cli.clipboard {
        overrides.capture.include_clipboard = Some(true);
    } else if cli.no_clipboard {
        overrides.capture.include_clipboard = Some(false);
    }
    if cli.screenshots {
        overrides.capture.include_screenshots = Some(true);
    } else if cli.no_screenshots {
        overrides.capture.include_screenshots = Some(false);
    }
    if cli.accessibility {
        overrides.capture.include_accessibility = Some(true);
    } else if cli.no_accessibility {
        overrides.capture.include_accessibility = Some(false);
    }
    if cli.actions {
        overrides.capture.include_actions = Some(true);
    } else if cli.no_actions {
        overrides.capture.include_actions = Some(false);
    }
    if let Some(value) = cli.accessibility_depth {
        overrides.capture.accessibility_depth = Some(value);
    }
    if let Some(value) = cli.image_quality {
        overrides.capture.image_quality = Some(value);
    }
    if let Some(value) = cli.screenshot_timeout_ms {
        overrides.capture.screenshot_timeout_ms = Some(value);
    }
    if let Some(value) = &cli.capture_dir {
        overrides.output.capture_dir = Some(value.clone());
    }
    if let Some(value) = &cli.state_file {
        overrides.output.state_file = Some(value.clone());
    }

    let cfg = config::load_with_options(
        config::default_app_name(),
        config::LoadOptions {
            cli_config: cli.config.clone(),
            overrides,
        },
    )?;
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

// ---------------------------------------------------------------------------
// Watch command
// ---------------------------------------------------------------------------

fn cmd_watch(
    dirs: &[std::path::PathBuf],
    no_clipboard: bool,
    interval_ms: u64,
    max_age_secs: u64,
    once: bool,
) -> anyhow::Result<()> {
    use std::time::Duration;

    let app_dirs = ctx_core::directories::AppDirectories::discover(config::default_app_name())?;
    let watch_dirs = if dirs.is_empty() {
        ingest::default_watch_dirs()
    } else {
        dirs.to_vec()
    };
    if watch_dirs.is_empty() && no_clipboard {
        anyhow::bail!("no screenshot directories found and clipboard polling disabled");
    }
    eprintln!(
        "watching {} (clipboard: {})",
        if watch_dirs.is_empty() {
            "no directories".to_string()
        } else {
            watch_dirs
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        },
        if no_clipboard { "off" } else { "on" }
    );

    let mut detector = ingest::ScreenshotDetector::new(ingest::DetectorOptions {
        watch_dirs,
        seen_index_path: app_dirs.state_dir.join("screenshot-seen.json"),
        inbox_dir: app_dirs.data_dir.join("inbox"),
        max_age: (max_age_secs > 0).then(|| Duration::from_secs(max_age_secs)),
    });
    let interval = Duration::from_millis(interval_ms.max(100));
    // The scanner needs two scans to consider a file settled, so a single
    // pass still polls twice with a short pause.
    let settle = Duration::from_millis(300);

    let emit = |event: &ingest::ScreenshotEvent| {
        if let Ok(line) = serde_json::to_string(event) {
            println!("{line}");
        }
    };

    loop {
        for event in detector.poll_files() {
            emit(&event);
        }
        if !no_clipboard
            && let Some((rgba, width, height)) = ingest::read_clipboard_image()
            && let Some(event) = detector.ingest_pixels(&rgba, width, height)?
        {
            emit(&event);
        }
        if once {
            std::thread::sleep(settle);
            for event in detector.poll_files() {
                emit(&event);
            }
            return Ok(());
        }
        std::thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Current-context command
// ---------------------------------------------------------------------------

fn cmd_current(cli: &Cli, json: bool, action: Option<&CurrentAction>) -> anyhow::Result<()> {
    let mut overrides = config::ConfigOverrides::default();
    if let Some(value) = &cli.state_file {
        overrides.output.state_file = Some(value.clone());
    }
    let cfg = config::load_with_options(
        config::default_app_name(),
        config::LoadOptions {
            cli_config: cli.config.clone(),
            overrides,
        },
    )?;

    let context = match action {
        Some(CurrentAction::Report {
            source,
            kind,
            app,
            bundle_id,
            window,
            workspace,
            url,
            cwd,
            project,
            depth,
            json: _,
        })
        | Some(CurrentAction::Set {
            source,
            kind,
            app,
            bundle_id,
            window,
            workspace,
            url,
            cwd,
            project,
            depth,
            json: _,
        }) => current::report_current_context(
            &cfg.output.state_file,
            CurrentContextReport {
                source: source.clone(),
                kind: kind.map(ContextKind::from),
                app: app.clone(),
                bundle_id: bundle_id.clone(),
                window: window.clone(),
                workspace: workspace.clone(),
                url: url.clone(),
                cwd: cwd.clone(),
                project: project.clone(),
                depth: *depth,
            },
        )?,
        None => current::read_current_context(&cfg.output.state_file)?,
    };

    let output_json = json
        || matches!(
            action,
            Some(CurrentAction::Report { json: true, .. } | CurrentAction::Set { json: true, .. })
        );

    if output_json {
        println!("{}", serde_json::to_string_pretty(&context)?);
    } else {
        println!("Current context");
        println!("State file: {}", cfg.output.state_file.display());
        println!("Sequence: {}", context.sequence);
        if let Ok(ts) = context
            .updated_at
            .format(&time::format_description::well_known::Rfc3339)
        {
            println!("Updated at: {ts}");
        }
        let active = &context.active;
        println!("Source: {}", active.source.as_deref().unwrap_or("<none>"));
        println!(
            "Kind: {}",
            active
                .kind
                .as_ref()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "<none>".to_string())
        );
        println!("App: {}", active.app.as_deref().unwrap_or("<none>"));
        println!(
            "Bundle ID: {}",
            active.bundle_id.as_deref().unwrap_or("<none>")
        );
        println!("Window: {}", active.window.as_deref().unwrap_or("<none>"));
        println!(
            "Workspace: {}",
            active.workspace.as_deref().unwrap_or("<none>")
        );
        println!("URL: {}", active.url.as_deref().unwrap_or("<none>"));
        println!("CWD: {}", active.cwd.as_deref().unwrap_or("<none>"));
        println!("Project: {}", active.project.as_deref().unwrap_or("<none>"));
        println!(
            "Depth: {}",
            active
                .depth
                .map(|depth| depth.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

fn cmd_config_path() -> anyhow::Result<()> {
    let path = config::config_file_path(config::default_app_name())?;
    println!("{}", path.display());
    Ok(())
}

fn cmd_config_show() -> anyhow::Result<()> {
    let path = config::config_file_path(config::default_app_name())?;
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        println!("# {}\n", path.display());
        print!("{content}");
    } else {
        println!("No config file found at {}", path.display());
        println!("Run `ctx config -i` to create one interactively.");
    }
    Ok(())
}

fn cmd_config_interactive() -> anyhow::Result<()> {
    use dialoguer::{Confirm, Input};

    let app_name = config::default_app_name();
    let cfg = config::load(app_name)?;

    println!("Interactive configuration for ctx");
    println!("Press Enter to keep current values.\n");

    // --- Capture settings ---
    println!("-- Capture --\n");

    let include_clipboard = Confirm::new()
        .with_prompt("Capture clipboard contents?")
        .default(cfg.capture.include_clipboard)
        .interact()?;

    let include_screenshots = Confirm::new()
        .with_prompt("Capture screenshots?")
        .default(cfg.capture.include_screenshots)
        .interact()?;

    let include_accessibility = Confirm::new()
        .with_prompt("Capture accessibility tree?")
        .default(cfg.capture.include_accessibility)
        .interact()?;

    let include_actions = Confirm::new()
        .with_prompt("Enable action layer?")
        .default(cfg.capture.include_actions)
        .interact()?;

    let accessibility_depth: u8 = Input::new()
        .with_prompt("Accessibility tree depth (0-16)")
        .default(cfg.capture.accessibility_depth)
        .validate_with(|input: &u8| {
            if *input <= 16 {
                Ok(())
            } else {
                Err("Must be between 0 and 16")
            }
        })
        .interact_text()?;

    let image_quality: u8 = Input::new()
        .with_prompt("Screenshot image quality (0-100)")
        .default(cfg.capture.image_quality)
        .validate_with(|input: &u8| {
            if *input <= 100 {
                Ok(())
            } else {
                Err("Must be between 0 and 100")
            }
        })
        .interact_text()?;

    let screenshot_timeout_ms: u64 = Input::new()
        .with_prompt("Screenshot timeout (ms)")
        .default(cfg.capture.screenshot_timeout_ms)
        .interact_text()?;

    // --- Output settings ---
    println!("\n-- Output --\n");

    let capture_dir: String = Input::new()
        .with_prompt("Capture output directory")
        .default(cfg.output.capture_dir.to_string_lossy().into_owned())
        .interact_text()?;

    let state_file: String = Input::new()
        .with_prompt("State file path")
        .default(cfg.output.state_file.to_string_lossy().into_owned())
        .interact_text()?;

    // --- AI provider settings ---
    println!("\n-- AI Provider (for future LLM-powered features) --\n");

    let default_provider: String = Input::new()
        .with_prompt("Default AI provider, e.g. openai, anthropic (leave empty for none)")
        .default(cfg.providers.default_provider.clone().unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;

    // --- Confirm and save ---
    println!();
    let save = Confirm::new()
        .with_prompt("Save configuration?")
        .default(true)
        .interact()?;

    if save {
        let new_cfg = config::AppConfig {
            directories: cfg.directories.clone(),
            capture: config::CaptureConfig {
                include_clipboard,
                include_screenshots,
                include_accessibility,
                include_actions,
                accessibility_depth,
                image_quality,
                screenshot_timeout_ms,
            },
            providers: config::ProviderConfig {
                default_provider: if default_provider.is_empty() {
                    None
                } else {
                    Some(default_provider)
                },
                api_keys: cfg.providers.api_keys.clone(),
            },
            output: config::OutputPaths {
                capture_dir: std::path::PathBuf::from(&capture_dir),
                state_file: std::path::PathBuf::from(&state_file),
                bundle_dir: cfg.output.bundle_dir.clone(),
            },
            ocr: cfg.ocr.clone(),
        };

        config::save_config(app_name, &new_cfg)?;
        let path = config::config_file_path(app_name)?;
        println!("Configuration saved to {}", path.display());
    } else {
        println!("Configuration not saved.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

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
    print_apps(&result.apps);
    print_windows(&result.windows);

    println!(
        "Clipboard: {}",
        if result.clipboard.enabled {
            "enabled"
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
            format!("{} capture(s)", result.screenshots.captures.len())
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
            "enabled"
        } else {
            "disabled"
        },
        result.accessibility.depth
    );
    if let Some(note) = &result.accessibility.note {
        println!("  Note: {note}");
    }
    if let Some(node) = &result.accessibility.focused {
        println!(
            "  Focused: role={role} label={label} value={value}",
            role = node.role.as_deref().unwrap_or("<unknown>"),
            label = node.label.as_deref().unwrap_or("<none>"),
            value = node.value.as_deref().unwrap_or("<none>")
        );
    }

    println!(
        "Actions: {}",
        if result.actions.enabled {
            if result.actions.supported {
                "enabled"
            } else {
                "enabled (unsupported)"
            }
        } else {
            "disabled"
        }
    );
    if let Some(note) = &result.actions.note {
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

fn print_displays(displays: &[DisplayInfo]) {
    if displays.is_empty() {
        println!("Displays: none reported");
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
        println!("Windows: none reported");
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

fn print_apps(apps: &[AppInfo]) {
    if apps.is_empty() {
        println!("Apps: none reported");
        return;
    }

    println!("Apps:");
    for app in apps {
        println!(
            "- {name} {focused}",
            name = app.name.as_deref().unwrap_or("<unknown app>"),
            focused = if app.focused { "(focused)" } else { "" }
        );
        if let Some(bundle_id) = &app.bundle_id {
            println!("  Bundle ID: {bundle_id}");
        }
        if let Some(pid) = app.pid {
            println!("  PID: {pid}");
        }
    }
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

        let file_name = saved.file_name().unwrap().to_string_lossy().to_string();
        assert!(file_name.contains(&saved_envelope.metadata.session_id));
        assert!(saved.starts_with(&cfg.output.capture_dir));
    }
}
