//! Selection state: what the user currently has highlighted, set by whatever
//! tool owns it (yazi, Finder, an agent) and read by command planning.
//!
//! Producers call `ctx selection set --file ...`; ctx never guesses from the
//! screen, never reads the clipboard and never follows symlinks out of the
//! selection it was given.
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Bounded so a runaway producer cannot fill the state file.
pub const MAX_FILES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    pub files: Vec<PathBuf>,
    /// Who set it: `yazi`, `finder`, `agent`, ...
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

impl Selection {
    pub fn empty() -> Self {
        Self {
            files: Vec::new(),
            source: "none".into(),
            at: OffsetDateTime::now_utc(),
        }
    }
}

pub fn path() -> Result<PathBuf> {
    let state = crate::directories::AppDirectories::discover("ctx")?
        .state_dir
        .join("selection.json");
    Ok(state)
}

/// Validate and store a producer's selection. Absolute paths only; the list is
/// deduplicated and bounded. Missing paths are kept (a file may be moved) but
/// reported by the caller through `missing`.
pub fn set(path: &Path, files: Vec<PathBuf>, source: &str) -> Result<Selection> {
    if files.len() > MAX_FILES {
        bail!("selection limited to {MAX_FILES} files");
    }
    let mut files = files;
    files.retain(|file| file.is_absolute());
    files.dedup();
    let selection = Selection {
        files,
        source: source.to_string(),
        at: OffsetDateTime::now_utc(),
    };
    let parent = path.parent().context("selection path needs a parent")?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".selection.json.{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = options
        .open(&temp)
        .context("cannot create selection file")?;
    file.write_all(serde_json::to_vec_pretty(&selection)?.as_slice())?;
    file.sync_all()?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })?;
    Ok(selection)
}

pub fn read(path: &Path) -> Result<Selection> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Selection::empty()),
        Err(error) => Err(error).context("cannot read selection file"),
    }
}

pub fn clear(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("cannot clear selection file"),
    }
}

impl Selection {
    /// Files that no longer exist, for an honest preview.
    pub fn missing(&self) -> Vec<&Path> {
        self.files
            .iter()
            .map(PathBuf::as_path)
            .filter(|path| !path.exists())
            .collect()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ctx-selection-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("selection.json")
    }

    #[test]
    fn set_read_clear_round_trip_is_owner_only_and_bounded() {
        let path = scratch("roundtrip");
        let first = std::env::temp_dir().join("ctx-sel-a.txt");
        std::fs::write(&first, b"a").unwrap();
        let selection = set(
            &path,
            vec![first.clone(), first.clone(), PathBuf::from("relative/path")],
            "yazi",
        )
        .unwrap();
        assert_eq!(
            selection.files,
            vec![first.clone()],
            "absolute, deduplicated"
        );
        assert_eq!(read(&path).unwrap(), selection);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        clear(&path).unwrap();
        assert!(read(&path).unwrap().is_empty());

        let many: Vec<PathBuf> = (0..MAX_FILES + 1)
            .map(|i| PathBuf::from(format!("/tmp/{i}")))
            .collect();
        assert!(set(&path, many, "yazi").is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_files_are_reported_not_dropped() {
        let path = scratch("missing");
        let absent = std::env::temp_dir().join("ctx-sel-absent-file");
        let _ = std::fs::remove_file(&absent);
        let selection = set(&path, vec![absent.clone()], "finder").unwrap();
        assert_eq!(selection.missing(), vec![absent.as_path()]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
