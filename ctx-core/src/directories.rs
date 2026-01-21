use std::env;
use std::path::PathBuf;

use thiserror::Error;

/// Application-specific directories for configuration, data, and state.
#[derive(Debug, Clone)]
pub struct AppDirectories {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error("could not determine a base directory for application data")]
    MissingBaseDirectory,
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl AppDirectories {
    /// Discover platform-appropriate directories for the application and ensure they exist.
    pub fn discover(app_name: &str) -> Result<Self, DirectoryError> {
        let config_base = config_base_dir().ok_or(DirectoryError::MissingBaseDirectory)?;
        let data_base = data_base_dir().ok_or(DirectoryError::MissingBaseDirectory)?;
        let state_base = state_base_dir().unwrap_or_else(|| data_base.clone());

        let config_dir = ensure_dir(config_base.join(app_name))?;
        let data_dir = ensure_dir(data_base.join(app_name))?;
        let state_dir = ensure_dir(state_base.join(app_name))?;

        Ok(Self {
            config_dir,
            data_dir,
            state_dir,
        })
    }
}

fn ensure_dir(path: PathBuf) -> Result<PathBuf, DirectoryError> {
    std::fs::create_dir_all(&path).map_err(|source| DirectoryError::CreateDir {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).or_else(dirs::home_dir)
}

#[cfg(target_os = "windows")]
fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").map(PathBuf::from).or_else(dirs::home_dir)
}

#[cfg(not(target_os = "windows"))]
fn config_base_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .or_else(|| home_dir().map(|home| home.join(".config")))
}

#[cfg(target_os = "windows")]
fn config_base_dir() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
}

#[cfg(not(target_os = "windows"))]
fn data_base_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(dirs::data_dir)
        .or_else(|| home_dir().map(|home| home.join(".local/share")))
}

#[cfg(target_os = "windows")]
fn data_base_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(dirs::data_dir)
}

#[cfg(not(target_os = "windows"))]
fn state_base_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local/state")))
}

#[cfg(target_os = "windows")]
fn state_base_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(dirs::state_dir)
}
