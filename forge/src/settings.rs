//! forge's own settings (port, quiet times, poll interval): the defaults in `defaults.toml`, with any value replaced
//! by the per-PC settings file. These belong to the PC, not to a game, so they never live in a game's `forge.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The defaults, built into the binary so forge works with no settings file at all.
const DEFAULTS: &str = include_str!("../defaults.toml");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub server: Server,
    pub watch: Watch,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Watch {
    pub studio_quiet_ms: u64,
    pub disk_quiet_ms: u64,
    pub script_poll_ms: u64,
}

/// Where the per-PC settings file lives (on Windows, `%APPDATA%\forge\settings.toml`).
pub fn user_file() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("forge").join("settings.toml"))
}

/// The defaults with the settings file's values on top, if the file exists.
pub fn load(file: Option<&Path>) -> Result<Settings> {
    let mut merged: toml::Table = DEFAULTS
        .parse()
        .context("forge's built-in defaults are broken")?;
    if let Some(path) = file.filter(|p| p.is_file()) {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("can't read {}", path.display()))?;
        let overrides: toml::Table = text
            .parse()
            .with_context(|| format!("{} isn't valid TOML", path.display()))?;
        overlay(&mut merged, overrides);
    }
    merged.try_into().with_context(|| match file {
        Some(path) => format!("a setting in {} is wrong", path.display()),
        None => "forge's built-in defaults are broken".to_string(),
    })
}

/// Replaces values in `base` with those in `top`, table by table, so a file only needs the settings it changes.
fn overlay(base: &mut toml::Table, top: toml::Table) {
    for (key, value) in top {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(top_inner)) => {
                overlay(inner, top_inner)
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("forge-settings-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn defaults_load_without_a_file() {
        let s = load(None).unwrap();
        assert_eq!(s.server.port, 34900);
        assert_eq!(s.watch.script_poll_ms, 1000);
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let s = load(Some(Path::new("no/such/settings.toml"))).unwrap();
        assert_eq!(s, load(None).unwrap());
    }

    #[test]
    fn a_file_changes_only_what_it_names() {
        let path = temp_file("partial", "[server]\nport = 35000\n");
        let s = load(Some(&path)).unwrap();
        assert_eq!(s.server.port, 35000);
        assert_eq!(s.watch, load(None).unwrap().watch);
    }

    #[test]
    fn a_misspelled_setting_is_an_error() {
        let path = temp_file("typo", "[server]\nprot = 35000\n");
        let err = format!("{:#}", load(Some(&path)).unwrap_err());
        assert!(err.contains("prot"), "{err}");
    }

    #[test]
    fn a_wrong_type_is_an_error() {
        let path = temp_file("type", "[server]\nport = \"high\"\n");
        assert!(load(Some(&path)).is_err());
    }
}
