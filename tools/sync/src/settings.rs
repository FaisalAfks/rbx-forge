//! forge sync's sections of the settings file, `[sync]` and `[watch]`, with their defaults in `defaults.toml`.

use std::ops::Deref;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const DEFAULTS: &str = include_str!("../defaults.toml");

/// sync's sections, in the order `forge config` shows them.
pub const SECTIONS: &[&str] = &["sync", "watch"];

/// forge's settings as sync's commands use them: core's, plus sync's own sections.
pub struct Settings {
    pub core: forge_core::settings::Settings,
    pub sync: Sync,
    pub watch: Watch,
}

impl Deref for Settings {
    type Target = forge_core::settings::Settings;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sync {
    pub model_timeout_ms: u64,
    pub yield_every: u64,
    pub confirm_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Watch {
    pub studio_quiet_ms: u64,
    pub disk_quiet_ms: u64,
    pub script_poll_ms: u64,
}

/// sync's sections, read together so a misspelled key names its section.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sections {
    pub sync: Sync,
    pub watch: Watch,
}

impl Settings {
    pub fn from(core: forge_core::settings::Settings) -> Result<Settings> {
        let s: Sections = core.sections(SECTIONS)?;
        Ok(Settings {
            core,
            sync: s.sync,
            watch: s.watch,
        })
    }
}
