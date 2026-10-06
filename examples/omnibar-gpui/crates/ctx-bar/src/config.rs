use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use ctx_bar_design::BarDesign;
use serde::{Deserialize, Serialize};

use crate::model::{Fixture, Platform, Presentation};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: String,
    #[serde(deserialize_with = "canonical_selector")]
    pub design: BarDesign,
    #[serde(deserialize_with = "canonical_selector")]
    pub presentation: Presentation,
    pub platform: Platform,
    pub fixture: Fixture,
    pub auto_select_first: bool,
    pub timeout_seconds: u64,
    pub debounce_ms: u64,
    pub request_timeout_seconds: u64,
    pub width: f32,
    /// Maximum expansion height; idle height comes from BarDesign geometry.
    pub height: f32,
}
// config's enum deserializer accepts case-insensitive variants. These selectors
// must instead use the canonical IDs shared by CLI flags and the JSON schema.
fn canonical_selector<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = String::deserialize(deserializer)?;
    serde_json::from_value(value.into()).map_err(serde::de::Error::custom)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "Lumen Dark".into(),
            design: BarDesign::default(),
            presentation: Presentation::default(),
            platform: if cfg!(target_os = "windows") {
                Platform::Windows
            } else if cfg!(target_os = "linux") {
                Platform::Linux
            } else {
                Platform::Macos
            },
            fixture: Fixture::Desktop,
            auto_select_first: false,
            timeout_seconds: 3,
            debounce_ms: 220,
            request_timeout_seconds: 15,
            width: 680.0,
            height: 520.0,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !["Lumen Dark", "Lumen Light"].contains(&self.theme.as_str())
            || !(1..=30).contains(&self.timeout_seconds)
            || !(100..=2000).contains(&self.debounce_ms)
            || !(1..=30).contains(&self.request_timeout_seconds)
            || !self.width.is_finite()
            || !(480.0..=1200.0).contains(&self.width)
            || !self.height.is_finite()
            || !(240.0..=900.0).contains(&self.height)
        {
            bail!("Invalid omnibar config; check the shipped JSON schema");
        }
        Ok(())
    }
}
pub fn base_dir(xdg: Option<PathBuf>, home: PathBuf, fallback: &str) -> PathBuf {
    xdg.filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(fallback))
}
fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("HOME or USERPROFILE is required when XDG directories are unset")
}
pub fn config_path() -> Result<PathBuf> {
    Ok(base_dir(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        home()?,
        ".config",
    )
    .join("ctx/omnibar-gpui.toml"))
}
pub fn descriptor_path() -> Result<PathBuf> {
    Ok(base_dir(
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        home()?,
        ".local/state",
    )
    .join("ctx/omnibar-prototype.json"))
}
const DEFAULT_TOML: &str = include_str!("../../../examples/config.toml");
const CONFIG_SCHEMA: &str = include_str!("../../../examples/omnibar-gpui.schema.json");

/// Never rewrites an existing file; first-run creation uses create_new.
pub fn create_default(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("Config path needs a parent directory")?;
    fs::create_dir_all(parent)?;
    write_new(&parent.join("omnibar-gpui.schema.json"), CONFIG_SCHEMA)?;
    write_new(path, DEFAULT_TOML)
}

fn write_new(path: &Path, content: &str) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(content.as_bytes())?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn load_layers(
    global: &Path,
    local: Option<&Path>,
    explicit: Option<&Path>,
    cli: &[(String, String)],
) -> Result<Config> {
    let defaults = Config::default();
    let mut builder = config::Config::builder()
        .set_default("theme", defaults.theme)?
        .set_default("design", defaults.design.id())?
        .set_default(
            "presentation",
            serde_json::to_value(defaults.presentation)?
                .as_str()
                .unwrap_or("flat"),
        )?
        .set_default(
            "platform",
            serde_json::to_value(defaults.platform)?
                .as_str()
                .unwrap_or("macos"),
        )?
        .set_default("fixture", "desktop")?
        .set_default("auto_select_first", false)?
        .set_default("timeout_seconds", 3)?
        .set_default("debounce_ms", 220)?
        .set_default("request_timeout_seconds", 15)?
        .set_default("width", 680.0)?
        .set_default("height", 520.0)?
        .add_source(config::File::from(global).required(false));
    if let Some(path) = local {
        builder = builder.add_source(config::File::from(path).required(false))
    }
    builder = builder.add_source(
        config::Environment::with_prefix("CTX_BAR")
            .separator("__")
            .prefix_separator("__")
            .try_parsing(true),
    );
    if let Some(path) = explicit {
        builder = builder.add_source(config::File::from(path).required(true))
    }
    for (key, value) in cli {
        builder = builder.set_override(key.clone(), value.clone())?
    }
    let cfg: Config = builder.build()?.try_deserialize()?;
    cfg.validate()?;
    Ok(cfg)
}
pub fn load_or_create(args: impl IntoIterator<Item = String>) -> Result<Config> {
    let mut args = args.into_iter();
    let mut explicit = None;
    let mut overrides = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--config" => {
                explicit = Some(PathBuf::from(
                    args.next().context("--config requires a path")?,
                ))
            }
            "--auto-select-first" => overrides.push(("auto_select_first".into(), "true".into())),
            "--no-auto-select-first" => {
                overrides.push(("auto_select_first".into(), "false".into()))
            }
            "--theme" | "--design" | "--presentation" | "--platform" | "--fixture"
            | "--timeout-seconds" => {
                let value = args.next().context("Flag requires a value")?;
                overrides.push((flag.trim_start_matches("--").replace('-', "_"), value));
            }
            _ => bail!("Unknown ctx-bar option; use --help"),
        }
    }
    let global = config_path()?;
    if explicit.is_none() {
        create_default(&global)?
    }
    load_layers(
        &global,
        Some(Path::new("ctx-bar.toml")),
        explicit.as_deref(),
        &overrides,
    )
}

#[cfg(test)]
#[path = "config_design_tests.rs"]
mod design_tests;

// Filesystem locations and first-run persistence remain separate from the
// native design/schema contract exercised by design_tests.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xdg_and_fallback_paths_are_pure() {
        let home = PathBuf::from("/synthetic/home");
        assert_eq!(
            base_dir(
                Some(PathBuf::from("/synthetic/config")),
                home.clone(),
                ".config"
            ),
            PathBuf::from("/synthetic/config")
        );
        assert_eq!(
            base_dir(Some(PathBuf::from("relative")), home.clone(), ".config"),
            home.join(".config")
        );
        assert_eq!(
            base_dir(None, home.clone(), ".local/state"),
            home.join(".local/state")
        );
    }
    #[test]
    fn file_then_explicit_then_cli_precedence_and_invalid_config() {
        let dir = std::env::temp_dir().join(format!("ctx-bar-config-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let global = dir.join("global.toml");
        let explicit = dir.join("explicit.toml");
        fs::write(&global, "timeout_seconds = 4\n").unwrap();
        fs::write(&explicit, "timeout_seconds = 5\n").unwrap();
        assert_eq!(
            load_layers(
                &global,
                None,
                Some(&explicit),
                &[("timeout_seconds".into(), "6".into())]
            )
            .unwrap()
            .timeout_seconds,
            6
        );
        assert!(
            load_layers(
                &global,
                None,
                None,
                &[("timeout_seconds".into(), "0".into())]
            )
            .is_err()
        );
        let before = fs::read(&global).unwrap();
        create_default(&global).unwrap();
        assert_eq!(fs::read(&global).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
}
