use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use config::{Config, Environment, File};
use serde::Deserialize;
use thiserror::Error;

use crate::directories::{AppDirectories, DirectoryError};

const APP_NAME: &str = "ctx";

/// Fully resolved configuration with concrete paths.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub directories: AppDirectories,
    pub capture: CaptureConfig,
    pub providers: ProviderConfig,
    pub output: OutputPaths,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_include_clipboard")]
    pub include_clipboard: bool,
    #[serde(default = "default_include_screenshots")]
    pub include_screenshots: bool,
    #[serde(default = "default_include_accessibility")]
    pub include_accessibility: bool,
    #[serde(default = "default_accessibility_depth")]
    pub accessibility_depth: u8,
    #[serde(default = "default_image_quality")]
    pub image_quality: u8,
    #[serde(default = "default_screenshot_timeout_ms")]
    pub screenshot_timeout_ms: u64,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            include_clipboard: default_include_clipboard(),
            include_screenshots: default_include_screenshots(),
            include_accessibility: default_include_accessibility(),
            accessibility_depth: default_accessibility_depth(),
            image_quality: default_image_quality(),
            screenshot_timeout_ms: default_screenshot_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub api_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputConfig {
    #[serde(default)]
    pub capture_dir: Option<String>,
    #[serde(default)]
    pub state_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutputPaths {
    pub capture_dir: PathBuf,
    pub state_file: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Directories(#[from] DirectoryError),
    #[error("failed to write default config at {path}: {source}")]
    WriteDefault {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to prepare path {path}: {source}")]
    CreatePath {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to build configuration: {0}")]
    Build(#[from] config::ConfigError),
    #[error("failed to expand path '{value}': {source}")]
    PathExpansion {
        value: String,
        source: shellexpand::LookupError<std::env::VarError>,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct FileConfig {
    #[serde(default)]
    capture: CaptureConfig,
    #[serde(default)]
    providers: ProviderConfig,
    #[serde(default)]
    output: OutputConfig,
}

/// Load application configuration, creating a default config.toml on first run.
pub fn load(app_name: &str) -> Result<AppConfig, ConfigError> {
    let directories = AppDirectories::discover(app_name)?;
    let config_path = directories.config_dir.join("config.toml");
    ensure_default_config(&config_path, &directories)?;

    let raw: Config = Config::builder()
        .add_source(File::from(config_path))
        .add_source(
            Environment::with_prefix("CTX")
                .separator("__")
                .try_parsing(true),
        )
        .build()?;

    let file_config: FileConfig = raw.try_deserialize()?;
    let output = resolve_output_paths(&file_config.output, &directories)?;

    Ok(AppConfig {
        directories,
        capture: file_config.capture,
        providers: file_config.providers,
        output,
    })
}

fn ensure_default_config(path: &Path, directories: &AppDirectories) -> Result<(), ConfigError> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreatePath {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let content = default_config_toml(directories);
    fs::write(path, content).map_err(|source| ConfigError::WriteDefault {
        path: path.to_path_buf(),
        source,
    })
}

fn resolve_output_paths(
    output: &OutputConfig,
    directories: &AppDirectories,
) -> Result<OutputPaths, ConfigError> {
    let capture_dir_raw = output
        .capture_dir
        .clone()
        .unwrap_or_else(|| default_capture_dir(directories));
    let state_file_raw = output
        .state_file
        .clone()
        .unwrap_or_else(|| default_state_file(directories));

    let capture_dir = expand_path(&capture_dir_raw)?;
    fs::create_dir_all(&capture_dir).map_err(|source| ConfigError::CreatePath {
        path: capture_dir.clone(),
        source,
    })?;

    let state_file = expand_path(&state_file_raw)?;
    if let Some(parent) = state_file.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreatePath {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    Ok(OutputPaths {
        capture_dir,
        state_file,
    })
}

fn expand_path(raw: &str) -> Result<PathBuf, ConfigError> {
    let expanded = shellexpand::full(raw).map_err(|source| ConfigError::PathExpansion {
        value: raw.to_owned(),
        source,
    })?;
    Ok(PathBuf::from(expanded.into_owned()))
}

fn default_config_toml(directories: &AppDirectories) -> String {
    let capture_dir = default_capture_dir(directories);
    let state_file = default_state_file(directories);

    format!(
        r#"# ctx configuration
# Paths expand ~ and environment variables like $XDG_CONFIG_HOME.

[capture]
include_clipboard = {include_clipboard}
include_screenshots = {include_screenshots}
include_accessibility = {include_accessibility}
accessibility_depth = {accessibility_depth}
image_quality = {image_quality}
screenshot_timeout_ms = {screenshot_timeout_ms}

[providers]
# default_provider = "openai"

[providers.api_keys]
# openai = "sk-..."
# anthropic = "api-key"

[output]
capture_dir = "{capture_dir}"
state_file = "{state_file}"
"#,
        include_clipboard = default_include_clipboard(),
        include_screenshots = default_include_screenshots(),
        include_accessibility = default_include_accessibility(),
        accessibility_depth = default_accessibility_depth(),
        image_quality = default_image_quality(),
        screenshot_timeout_ms = default_screenshot_timeout_ms(),
    )
}

fn default_capture_dir(directories: &AppDirectories) -> String {
    directories
        .data_dir
        .join("captures")
        .to_string_lossy()
        .into_owned()
}

fn default_state_file(directories: &AppDirectories) -> String {
    directories
        .state_dir
        .join("sessions.json")
        .to_string_lossy()
        .into_owned()
}

const fn default_include_clipboard() -> bool {
    false
}

const fn default_include_screenshots() -> bool {
    true
}

const fn default_include_accessibility() -> bool {
    true
}

const fn default_accessibility_depth() -> u8 {
    3
}

const fn default_image_quality() -> u8 {
    85
}

const fn default_screenshot_timeout_ms() -> u64 {
    2_000
}

/// Convenience for the default app name used across crates.
pub fn default_app_name() -> &'static str {
    APP_NAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    struct EnvGuard {
        previous: Vec<(String, Option<OsString>)>,
    }

    impl EnvGuard {
        fn set(vars: &[(&str, &Path)]) -> Self {
            let mut previous = Vec::with_capacity(vars.len());
            for (key, value) in vars {
                previous.push((key.to_string(), env::var_os(key)));
                unsafe {
                    env::set_var(key, value);
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, val) in self.previous.drain(..).rev() {
                unsafe {
                    match val {
                        Some(v) => env::set_var(&key, v),
                        None => env::remove_var(&key),
                    }
                }
            }
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_creates_default_config_and_dirs() {
        let _lock = env_lock().lock().unwrap();
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path();

        let _guard = EnvGuard::set(&[
            ("XDG_CONFIG_HOME", base.join("config").as_path()),
            ("XDG_DATA_HOME", base.join("data").as_path()),
            ("XDG_STATE_HOME", base.join("state").as_path()),
        ]);

        let cfg = load(default_app_name()).expect("load config");
        let config_path = cfg.directories.config_dir.join("config.toml");
        assert!(config_path.exists(), "config file should be created");

        let capture_dir = cfg.directories.data_dir.join("captures");
        assert_eq!(cfg.output.capture_dir, capture_dir);
        assert!(cfg.output.capture_dir.exists(), "capture dir created");

        let state_file = cfg.directories.state_dir.join("sessions.json");
        assert_eq!(cfg.output.state_file, state_file);
        assert!(
            cfg.output.state_file.parent().unwrap().exists(),
            "state dir created"
        );
    }

    #[test]
    fn expands_tilde_and_env_vars_in_paths() {
        let _lock = env_lock().lock().unwrap();
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path();

        let _guard = EnvGuard::set(&[
            ("HOME", base),
            ("XDG_CONFIG_HOME", base.join("config2").as_path()),
            ("XDG_DATA_HOME", base.join("data2").as_path()),
            ("XDG_STATE_HOME", base.join("state2").as_path()),
        ]);

        let app_dir = base.join("config2").join(default_app_name());
        std::fs::create_dir_all(&app_dir).expect("create app dir");
        let config_path = app_dir.join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[output]
capture_dir = "~/.local/share/ctx/custom_caps"
state_file = "~/.local/state/ctx/custom_state.json"
"#,
        )
        .expect("write config");

        let cfg = load(default_app_name()).expect("load config");

        let expected_capture = base.join(".local/share/ctx/custom_caps");
        assert_eq!(cfg.output.capture_dir, expected_capture);
        assert!(cfg.output.capture_dir.exists(), "capture dir created");

        let expected_state = base.join(".local/state/ctx/custom_state.json");
        assert_eq!(cfg.output.state_file, expected_state);
        assert!(
            cfg.output.state_file.parent().unwrap().exists(),
            "state dir created"
        );
    }
}
