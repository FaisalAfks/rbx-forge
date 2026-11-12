//! forge's own settings (port, quiet times, poll interval): the defaults in `core/defaults.toml` and each tool's
//! `defaults.toml`, with any value replaced by the per-PC settings file. These belong to the PC, not to a game, so they
//! never live in a game's `forge.toml`.
//!
//! Core reads `[server]` and `[plugin]`. Every other section belongs to a tool, which reads and checks it
//! (`Tool::settings_toml`); a section no tool owns, or a misspelled key in any section, is an error for every command.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::tool::Tool;

/// Core's defaults, built into the binary so forge works with no settings file at all.
const DEFAULTS: &str = include_str!("../defaults.toml");

/// Core's sections, in the order `forge config` shows them.
const SECTIONS: &[&str] = &["server", "plugin"];

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub server: Server,
    pub plugin: Plugin,
    /// Every section with defaults filled in, tools' included, for tools to read.
    all: toml::Table,
    /// The file the values came from, for error messages.
    file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub port: u16,
    pub start_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub max_message_mb: usize,
}

impl Server {
    pub fn max_message_bytes(&self) -> usize {
        self.max_message_mb.saturating_mul(1024 * 1024)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub retry_ms: u64,
    pub open_timeout_ms: u64,
}

/// Core's own sections, read together so a misspelled key names its section.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Core {
    server: Server,
    plugin: Plugin,
}

impl Settings {
    /// A tool's sections, read as `T` (a struct with one field per section).
    pub fn sections<T: DeserializeOwned>(&self, names: &[&str]) -> Result<T> {
        let table: toml::Table = names
            .iter()
            .filter_map(|n| Some((n.to_string(), self.all.get(*n)?.clone())))
            .collect();
        table.try_into().with_context(|| self.wrong())
    }

    /// Core's sections as `forge config` shows them.
    pub fn core_toml(&self) -> Result<String> {
        let core = Core {
            server: self.server.clone(),
            plugin: self.plugin.clone(),
        };
        toml::to_string(&core).context("can't show the settings")
    }

    fn wrong(&self) -> String {
        wrong(self.file.as_deref())
    }
}

fn wrong(file: Option<&Path>) -> String {
    match file {
        Some(path) => format!("a setting in {} is wrong", path.display()),
        None => "forge's built-in defaults are broken".to_string(),
    }
}

/// Names a settings file to use instead of the usual one. Tests use it to run their own server on their own port, and
/// a server that a command starts inherits it.
pub const FILE_VAR: &str = "FORGE_SETTINGS";

/// Where the per-PC settings file lives: `FORGE_SETTINGS` if set, else (on Windows) `%APPDATA%\forge\settings.toml`.
pub fn user_file() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(FILE_VAR).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    dirs::config_dir().map(|dir| dir.join("forge").join("settings.toml"))
}

/// The defaults (core's and every tool's) with the settings file's values on top, if the file exists, checked by
/// core and by every tool.
pub fn load(file: Option<&Path>, tools: &[&dyn Tool]) -> Result<Settings> {
    let mut merged = parse_defaults(DEFAULTS, "core")?;
    for tool in tools {
        for (key, value) in parse_defaults(tool.settings_defaults(), tool.name())? {
            if merged.insert(key.clone(), value).is_some() {
                bail!(
                    "two tools have settings called [{key}] (the second is {})",
                    tool.name()
                );
            }
        }
    }
    let known: Vec<String> = merged.keys().cloned().collect();
    let file = file.filter(|p| p.is_file());
    if let Some(path) = file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("can't read {}", path.display()))?;
        let overrides: toml::Table = text
            .parse()
            .with_context(|| format!("{} isn't valid TOML", path.display()))?;
        if let Some(key) = overrides.keys().find(|k| !known.contains(k)) {
            let order: Vec<&str> = SECTIONS
                .iter()
                .copied()
                .chain(tools.iter().flat_map(|t| tool_sections(*t)))
                .collect();
            bail!(
                "{}: unknown section `{key}`, expected one of {}",
                wrong(Some(path)),
                crate::game::quoted(&order)
            );
        }
        overlay(&mut merged, overrides);
    }
    let core: toml::Table = SECTIONS
        .iter()
        .filter_map(|n| Some((n.to_string(), merged.get(*n)?.clone())))
        .collect();
    let core: Core = core.try_into().with_context(|| wrong(file))?;
    let settings = Settings {
        server: core.server,
        plugin: core.plugin,
        all: merged,
        file: file.map(Path::to_path_buf),
    };
    for tool in tools {
        tool.settings_toml(&settings)?;
    }
    Ok(settings)
}

/// A tool's sections, in the order its defaults file has them.
pub fn tool_sections(tool: &dyn Tool) -> Vec<&'static str> {
    let defaults = tool.settings_defaults();
    defaults
        .lines()
        .filter_map(|l| l.trim().strip_prefix('[')?.strip_suffix(']'))
        .collect()
}

fn parse_defaults(text: &str, owner: &str) -> Result<toml::Table> {
    text.parse()
        .with_context(|| format!("{owner}'s built-in defaults are broken"))
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
        let s = load(None, &[]).unwrap();
        assert_eq!(s.server.port, 34900);
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let s = load(Some(Path::new("no/such/settings.toml")), &[]).unwrap();
        assert_eq!(s, load(None, &[]).unwrap());
    }

    #[test]
    fn a_file_changes_only_what_it_names() {
        let path = temp_file("partial", "[server]\nport = 35000\n");
        let s = load(Some(&path), &[]).unwrap();
        assert_eq!(s.server.port, 35000);
        assert_eq!(s.plugin, load(None, &[]).unwrap().plugin);
    }

    #[test]
    fn a_misspelled_setting_is_an_error() {
        let path = temp_file("typo", "[server]\nprot = 35000\n");
        let err = format!("{:#}", load(Some(&path), &[]).unwrap_err());
        assert!(err.contains("prot"), "{err}");
    }

    #[test]
    fn a_section_no_tool_owns_is_an_error() {
        let path = temp_file("section", "[lint]\nx = 1\n");
        let err = format!("{:#}", load(Some(&path), &[]).unwrap_err());
        assert!(err.contains("unknown section `lint`"), "{err}");
    }

    #[test]
    fn a_wrong_type_is_an_error() {
        let path = temp_file("type", "[server]\nport = \"high\"\n");
        assert!(load(Some(&path), &[]).is_err());
    }
}
