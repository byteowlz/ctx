//! Contacts and channels for argument-taking commands ("send this to Anna").
//!
//! Private, local configuration: `$XDG_CONFIG_HOME/ctx/contacts.toml`. A person
//! maps to one or more channel handles plus a preferred channel. Nothing here
//! sends anything; it only resolves names so a plan can be previewed honestly.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Person {
    pub name: String,
    /// Extra spellings the ASR or the user may produce.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Channel id to that channel's handle, e.g. `teams = "anna@example.com"`.
    #[serde(default)]
    pub channels: BTreeMap<String, String>,
    /// Preferred channel; falls back to `[commands] default_channel`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub person: Vec<Person>,
    #[serde(default)]
    pub commands: Defaults,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Defaults {
    /// Used when a person has no `preferred` channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel: Option<String>,
    /// Send is preview-only unless this is explicitly true. It stays false:
    /// there is no channel adapter yet, and sending files is irreversible.
    #[serde(default)]
    pub allow_send: bool,
}

pub fn path() -> Result<PathBuf> {
    Ok(crate::config::config_file_path("ctx")?.with_file_name("contacts.toml"))
}

pub fn load(path: &Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).context("contacts.toml is malformed"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(error).context("cannot read contacts.toml"),
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Config {
    /// Resolve a spoken name to one person. Ambiguous or unknown names resolve
    /// to nothing: a plan must show exactly who it means.
    pub fn find(&self, spoken: &str) -> Found {
        let wanted = normalize(spoken);
        if wanted.is_empty() {
            return Found::Unknown;
        }
        let matches: Vec<&Person> = self
            .person
            .iter()
            .filter(|person| {
                normalize(&person.name) == wanted
                    || person.aliases.iter().any(|alias| normalize(alias) == wanted)
                    || normalize(&person.name).starts_with(&wanted)
                    || person
                        .aliases
                        .iter()
                        .any(|alias| normalize(alias).starts_with(&wanted))
                    // A surname or a middle word is enough when it is unique.
                    || normalize(&person.name)
                        .split_whitespace()
                        .any(|word| word.starts_with(&wanted))
            })
            .collect();
        match matches.as_slice() {
            [] => Found::Unknown,
            [one] => Found::One((*one).clone()),
            many => Found::Ambiguous(many.iter().map(|p| p.name.clone()).collect()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "result")]
pub enum Found {
    One(Person),
    Ambiguous(Vec<String>),
    Unknown,
}

impl Person {
    /// Channel and handle for this person, with the configured fallback.
    pub fn channel(&self, defaults: &Defaults) -> Option<(String, String)> {
        let preferred = self
            .preferred
            .clone()
            .or_else(|| defaults.default_channel.clone())?;
        let handle = self.channels.get(&preferred)?.clone();
        Some((preferred, handle))
    }
}

pub fn validate(config: &Config) -> Result<()> {
    for person in &config.person {
        if person.name.trim().is_empty() {
            bail!("a contact has an empty name");
        }
        if let Some(preferred) = &person.preferred
            && !person.channels.contains_key(preferred)
        {
            bail!(
                "{} prefers channel {preferred} but has no handle for it",
                person.name
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        toml::from_str(
            r#"
[commands]
default_channel = "teams"

[[person]]
name = "Anna Müller"
aliases = ["Anna", "Anja"]
preferred = "teams"
[person.channels]
teams = "anna@example.com"
mail = "anna.mueller@example.com"

[[person]]
name = "Anna Schmidt"
aliases = ["Anna S"]
[person.channels]
teams = "anna.schmidt@example.com"

[[person]]
name = "Paul"
[person.channels]
teams = "paul@example.com"
"#,
        )
        .unwrap()
    }

    #[test]
    fn resolves_unique_names_aliases_and_reports_ambiguity() {
        validate(&config()).unwrap();
        match config().find("paul") {
            Found::One(person) => {
                assert_eq!(person.name, "Paul");
                assert_eq!(
                    person.channel(&config().commands),
                    Some(("teams".into(), "paul@example.com".into()))
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(config().find("Schmidt"), Found::One(_)));
        // "Anna" alone matches both Annas: never guess.
        assert!(matches!(config().find("anna"), Found::Ambiguous(_)));
        assert!(matches!(config().find("nobody"), Found::Unknown));
        assert!(matches!(config().find(""), Found::Unknown));
    }

    #[test]
    fn a_preferred_channel_must_have_a_handle() {
        let broken: Config = toml::from_str(
            r#"
[[person]]
name = "Anna"
preferred = "teams"
[person.channels]
mail = "a@b"
"#,
        )
        .unwrap();
        assert!(validate(&broken).is_err());
    }

    #[test]
    fn missing_or_empty_config_is_not_an_error() {
        let path = std::env::temp_dir().join("ctx-contacts-absent.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path).unwrap(), Config::default());
        assert!(matches!(load(&path).unwrap().find("anna"), Found::Unknown));
    }
}
