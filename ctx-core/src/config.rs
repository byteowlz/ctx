use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
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
    pub ocr: OcrConfig,
}

#[derive(Debug, Default, Clone)]
pub struct LoadOptions {
    pub cli_config: Option<PathBuf>,
    pub overrides: ConfigOverrides,
}

#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    pub capture: CaptureOverrides,
    pub output: OutputOverrides,
}

#[derive(Debug, Default, Clone)]
pub struct CaptureOverrides {
    pub include_clipboard: Option<bool>,
    pub include_screenshots: Option<bool>,
    pub include_accessibility: Option<bool>,
    pub include_actions: Option<bool>,
    pub accessibility_depth: Option<u8>,
    pub image_quality: Option<u8>,
    pub screenshot_timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Clone)]
pub struct OutputOverrides {
    pub capture_dir: Option<String>,
    pub state_file: Option<String>,
    pub bundle_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaptureConfig {
    #[serde(default = "default_include_clipboard")]
    pub include_clipboard: bool,
    #[serde(default = "default_include_screenshots")]
    pub include_screenshots: bool,
    #[serde(default = "default_include_accessibility")]
    pub include_accessibility: bool,
    #[serde(default = "default_include_actions")]
    pub include_actions: bool,
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
            include_actions: default_include_actions(),
            accessibility_depth: default_accessibility_depth(),
            image_quality: default_image_quality(),
            screenshot_timeout_ms: default_screenshot_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub api_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OutputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_dir: Option<String>,
}

/// OCR backend configuration for bundle image OCR.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrConfig {
    /// Command used to run OCR, e.g. `ocrs`.
    #[serde(default = "default_ocr_command")]
    pub command: String,
    /// Extra arguments inserted before the image path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Automatically OCR crops when they are created.
    #[serde(default = "default_ocr_auto_crops")]
    pub auto_crops: bool,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            command: default_ocr_command(),
            args: Vec::new(),
            auto_crops: default_ocr_auto_crops(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputPaths {
    pub capture_dir: PathBuf,
    pub state_file: PathBuf,
    pub bundle_dir: PathBuf,
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
    #[error("config file not found at {path}")]
    MissingCliConfig { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FileConfig {
    #[serde(
        rename = "$schema",
        default = "default_schema_url",
        skip_serializing_if = "String::is_empty"
    )]
    schema: String,
    #[serde(default)]
    capture: CaptureConfig,
    #[serde(default)]
    providers: ProviderConfig,
    #[serde(default)]
    output: OutputConfig,
    #[serde(default)]
    ocr: OcrConfig,
}

/// Load application configuration, creating a default config.toml on first run.
pub fn load(app_name: &str) -> Result<AppConfig, ConfigError> {
    load_with_options(app_name, LoadOptions::default())
}

/// Load application configuration with explicit precedence ordering.
pub fn load_with_options(app_name: &str, options: LoadOptions) -> Result<AppConfig, ConfigError> {
    let directories = AppDirectories::discover(app_name)?;
    let config_path = directories.config_dir.join("config.toml");
    ensure_default_config(&config_path, &directories)?;

    let mut builder = Config::builder().add_source(File::from(config_path));

    if let Some(local) = local_config_path(app_name)? {
        builder = builder.add_source(File::from(local));
    }

    builder = builder.add_source(
        Environment::with_prefix("CTX")
            .separator("__")
            .try_parsing(true),
    );

    if let Some(cli_config) = options.cli_config {
        let expanded = expand_path_value(cli_config)?;
        if !expanded.exists() {
            return Err(ConfigError::MissingCliConfig { path: expanded });
        }
        builder = builder.add_source(File::from(expanded));
    }

    let raw: Config = builder.build()?;

    let mut file_config: FileConfig = raw.try_deserialize()?;
    apply_overrides(&mut file_config, &options.overrides);
    let output = resolve_output_paths(&file_config.output, &directories)?;

    Ok(AppConfig {
        directories,
        capture: file_config.capture,
        providers: file_config.providers,
        output,
        ocr: file_config.ocr,
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

    let bundle_dir_raw = output
        .bundle_dir
        .clone()
        .unwrap_or_else(|| default_bundle_dir(directories));
    let bundle_dir = expand_path(&bundle_dir_raw)?;
    fs::create_dir_all(&bundle_dir).map_err(|source| ConfigError::CreatePath {
        path: bundle_dir.clone(),
        source,
    })?;

    Ok(OutputPaths {
        capture_dir,
        state_file,
        bundle_dir,
    })
}

fn expand_path(raw: &str) -> Result<PathBuf, ConfigError> {
    let expanded = shellexpand::full(raw).map_err(|source| ConfigError::PathExpansion {
        value: raw.to_owned(),
        source,
    })?;
    Ok(PathBuf::from(expanded.into_owned()))
}

fn expand_path_value(path: PathBuf) -> Result<PathBuf, ConfigError> {
    let raw = path.to_string_lossy().to_string();
    expand_path(&raw)
}

fn local_config_path(app_name: &str) -> Result<Option<PathBuf>, ConfigError> {
    let cwd = std::env::current_dir().map_err(|source| ConfigError::CreatePath {
        path: PathBuf::from("."),
        source,
    })?;
    let local = cwd.join(format!("{app_name}.toml"));
    if local.exists() {
        return Ok(Some(local));
    }
    Ok(None)
}

fn apply_overrides(file_config: &mut FileConfig, overrides: &ConfigOverrides) {
    let capture = &overrides.capture;
    if let Some(value) = capture.include_clipboard {
        file_config.capture.include_clipboard = value;
    }
    if let Some(value) = capture.include_screenshots {
        file_config.capture.include_screenshots = value;
    }
    if let Some(value) = capture.include_accessibility {
        file_config.capture.include_accessibility = value;
    }
    if let Some(value) = capture.include_actions {
        file_config.capture.include_actions = value;
    }
    if let Some(value) = capture.accessibility_depth {
        file_config.capture.accessibility_depth = value;
    }
    if let Some(value) = capture.image_quality {
        file_config.capture.image_quality = value;
    }
    if let Some(value) = capture.screenshot_timeout_ms {
        file_config.capture.screenshot_timeout_ms = value;
    }

    let output = &overrides.output;
    if let Some(value) = &output.capture_dir {
        file_config.output.capture_dir = Some(value.clone());
    }
    if let Some(value) = &output.state_file {
        file_config.output.state_file = Some(value.clone());
    }
    if let Some(value) = &output.bundle_dir {
        file_config.output.bundle_dir = Some(value.clone());
    }
}

fn default_config_toml(directories: &AppDirectories) -> String {
    let capture_dir = default_capture_dir(directories);
    let state_file = default_state_file(directories);
    let bundle_dir = default_bundle_dir(directories);

    format!(
        r#""$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/ctx/ctx.config.schema.json"

# ctx configuration
# Paths expand ~ and environment variables like $XDG_CONFIG_HOME.

[capture]
include_clipboard = {include_clipboard}
include_screenshots = {include_screenshots}
include_accessibility = {include_accessibility}
include_actions = {include_actions}
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
bundle_dir = "{bundle_dir}"

[ocr]
# Command used to run OCR on bundle images (v1 shells out to the `ocrs` CLI).
command = "{ocr_command}"
# Automatically OCR crops when they are created.
auto_crops = {ocr_auto_crops}
# Extra args inserted before the image path, e.g. []
args = []
"#,
        include_clipboard = default_include_clipboard(),
        include_screenshots = default_include_screenshots(),
        include_accessibility = default_include_accessibility(),
        include_actions = default_include_actions(),
        accessibility_depth = default_accessibility_depth(),
        image_quality = default_image_quality(),
        screenshot_timeout_ms = default_screenshot_timeout_ms(),
        ocr_command = default_ocr_command(),
        ocr_auto_crops = default_ocr_auto_crops(),
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

fn default_bundle_dir(directories: &AppDirectories) -> String {
    directories
        .data_dir
        .join("bundles")
        .to_string_lossy()
        .into_owned()
}

fn default_ocr_command() -> String {
    "ocrs".to_string()
}

const fn default_ocr_auto_crops() -> bool {
    true
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

const fn default_include_actions() -> bool {
    false
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

fn default_schema_url() -> String {
    "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/ctx/ctx.config.schema.json"
        .to_owned()
}

/// Save configuration values back to the global config.toml.
///
/// Preserves the `$schema` reference and writes capture, providers, and output
/// sections.
pub fn save_config(app_name: &str, cfg: &AppConfig) -> Result<(), ConfigError> {
    let config_path = cfg.directories.config_dir.join("config.toml");

    let output_config = OutputConfig {
        capture_dir: Some(cfg.output.capture_dir.to_string_lossy().into_owned()),
        state_file: Some(cfg.output.state_file.to_string_lossy().into_owned()),
        bundle_dir: Some(cfg.output.bundle_dir.to_string_lossy().into_owned()),
    };

    let file_config = FileConfig {
        schema: default_schema_url(),
        capture: cfg.capture.clone(),
        providers: cfg.providers.clone(),
        output: output_config,
        ocr: cfg.ocr.clone(),
    };

    let content = toml::to_string_pretty(&file_config).map_err(|e| {
        ConfigError::WriteDefault {
            path: config_path.clone(),
            source: std::io::Error::other(e.to_string()),
        }
    })?;

    // Prepend a comment header
    let header = format!(
        "# {app_name} configuration\n# Paths expand ~ and environment variables like $XDG_CONFIG_HOME.\n\n"
    );
    let final_content = format!("{header}{content}");

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreatePath {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(&config_path, final_content).map_err(|source| ConfigError::WriteDefault {
        path: config_path,
        source,
    })?;

    Ok(())
}

/// Returns the path to the global config file.
pub fn config_file_path(app_name: &str) -> Result<PathBuf, ConfigError> {
    let directories = AppDirectories::discover(app_name)?;
    Ok(directories.config_dir.join("config.toml"))
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

        let bundle_dir = cfg.directories.data_dir.join("bundles");
        assert_eq!(cfg.output.bundle_dir, bundle_dir);
        assert!(cfg.output.bundle_dir.exists(), "bundle dir created");
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
