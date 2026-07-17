use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

pub const CURRENT_CONTEXT_SCHEMA: &str =
    "https://byteowlz.github.io/schemas/ctx/current-context.v1.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentContext {
    pub schema: String,
    pub version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub sequence: u64,
    pub active: ActiveContext,
}

impl Default for CurrentContext {
    fn default() -> Self {
        Self {
            schema: CURRENT_CONTEXT_SCHEMA.to_string(),
            version: 1,
            updated_at: OffsetDateTime::now_utc(),
            sequence: 0,
            active: ActiveContext::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ContextKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ContextKind {
    Application,
    Terminal,
    Browser,
    Editor,
    Unknown,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CurrentContextReport {
    pub source: Option<String>,
    pub kind: Option<ContextKind>,
    pub app: Option<String>,
    pub bundle_id: Option<String>,
    pub window: Option<String>,
    pub workspace: Option<String>,
    pub url: Option<String>,
    pub cwd: Option<String>,
    pub project: Option<String>,
}

impl CurrentContext {
    #[must_use]
    pub fn apply_report(mut self, report: CurrentContextReport) -> Self {
        self.active = ActiveContext {
            source: report.source,
            kind: report.kind,
            app: report.app,
            bundle_id: report.bundle_id,
            window: report.window,
            workspace: report.workspace,
            url: report.url,
            cwd: report.cwd,
            project: report.project,
        };
        self.updated_at = OffsetDateTime::now_utc();
        self.sequence = self.sequence.saturating_add(1);
        self
    }
}

#[derive(Debug, Error)]
pub enum CurrentContextError {
    #[error("failed to read current context from {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse current context from {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to serialize current context: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to write temporary state file {path}: {source}")]
    WriteTemp {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to atomically replace state file {path}: {source}")]
    Rename {
        path: String,
        source: std::io::Error,
    },
}

pub fn read_current_context(path: &Path) -> Result<CurrentContext, CurrentContextError> {
    if !path.exists() {
        return Ok(CurrentContext::default());
    }
    let payload = fs::read_to_string(path).map_err(|source| CurrentContextError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&payload).map_err(|source| CurrentContextError::Parse {
        path: path.display().to_string(),
        source,
    })
}

pub fn report_current_context(
    path: &Path,
    report: CurrentContextReport,
) -> Result<CurrentContext, CurrentContextError> {
    let current = read_current_context(path)?.apply_report(report);
    write_current_context_atomic(path, &current)?;
    Ok(current)
}

pub fn write_current_context_atomic(
    path: &Path,
    context: &CurrentContext,
) -> Result<(), CurrentContextError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CurrentContextError::CreateDir {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let mut temp = path.to_path_buf();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("current-context.json");
    temp.set_file_name(format!(".{file_name}.tmp-{}", std::process::id()));

    let payload = serde_json::to_vec_pretty(context)?;
    fs::write(&temp, payload).map_err(|source| CurrentContextError::WriteTemp {
        path: temp.display().to_string(),
        source,
    })?;
    fs::rename(&temp, path).map_err(|source| CurrentContextError::Rename {
        path: path.display().to_string(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_replaces_active_context_and_writes_state_atomically() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("ctx").join("current-context.json");

        let terminal = report_current_context(
            &path,
            CurrentContextReport {
                source: Some("tmux".to_string()),
                kind: Some(ContextKind::Terminal),
                app: Some("Ghostty".to_string()),
                window: Some("ctx".to_string()),
                cwd: Some("/tmp/example".to_string()),
                project: Some("example".to_string()),
                ..Default::default()
            },
        )
        .expect("terminal report");

        assert_eq!(terminal.sequence, 1);
        assert_eq!(terminal.active.cwd.as_deref(), Some("/tmp/example"));
        assert!(path.exists());

        let browser = report_current_context(
            &path,
            CurrentContextReport {
                source: Some("aerospace".to_string()),
                kind: Some(ContextKind::Application),
                app: Some("Safari".to_string()),
                window: Some("Research".to_string()),
                workspace: Some("web".to_string()),
                url: Some("https://example.com/research".to_string()),
                ..Default::default()
            },
        )
        .expect("browser report");

        assert_eq!(browser.sequence, 2);
        assert_eq!(browser.active.source.as_deref(), Some("aerospace"));
        assert_eq!(browser.active.app.as_deref(), Some("Safari"));
        assert_eq!(browser.active.workspace.as_deref(), Some("web"));
        assert_eq!(
            browser.active.url.as_deref(),
            Some("https://example.com/research")
        );
        assert_eq!(browser.active.cwd, None);
        assert_eq!(browser.active.project, None);

        let roundtrip = read_current_context(&path).expect("read");
        assert_eq!(roundtrip, browser);
    }
}
