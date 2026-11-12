//! The interface every forge tool fills in (decision 0019). A tool's crate in `tools/<name>/` implements `Tool`, and
//! the binary, `forge mcp` and the plugin each list it once:
//! - its command (`forge <name> ...`) and what runs it;
//! - its sections of the settings file and its keys in a game's `forge.toml`, which it reads and checks itself;
//! - its MCP tools, each a forge command line built from the tool's arguments;
//! - its Studio side, `tools/<name>/studio/`, which the plugin packs and lists in `plugin/main.luau`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value as Json};

use crate::game::{self, Game};
use crate::protocol::Repo;
use crate::settings::{self, Settings};

pub trait Tool: Sync {
    /// Its name: its command (`forge sync`), its plugin folder (`tools/sync`) and the `tool` field of its requests.
    fn name(&self) -> &'static str;

    /// Its command, with its subcommands and arguments, named `name()`.
    fn command(&self) -> clap::Command;

    /// Runs its command.
    fn run(&self, cx: &Cx, matches: &clap::ArgMatches) -> Result<()>;

    /// Its sections of the settings file with their defaults, as TOML text (its `defaults.toml`).
    fn settings_defaults(&self) -> &'static str {
        ""
    }

    /// Reads and checks its sections, and gives them back as TOML for `forge config`.
    fn settings_toml(&self, _settings: &Settings) -> Result<String> {
        Ok(String::new())
    }

    /// The keys of a game's `forge.toml` it owns.
    fn game_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Reads and checks its keys of a game's `forge.toml`, and says what they set up, a line each, for `forge config`.
    fn game_lines(&self, _game: &Game) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Its MCP tools.
    fn mcp_tools(&self) -> Vec<McpTool> {
        Vec::new()
    }
}

/// What every command gets: the settings file in effect and the tools forge has.
pub struct Cx {
    pub settings_file: Option<PathBuf>,
    pub tools: &'static [&'static dyn Tool],
}

impl Cx {
    pub fn settings_file(&self) -> Option<&Path> {
        self.settings_file.as_deref()
    }

    /// The settings, checked by every tool.
    pub fn settings(&self) -> Result<Settings> {
        settings::load(self.settings_file(), self.tools)
    }

    /// The game around the current folder, if there is one.
    pub fn game_here(&self) -> Result<Option<Game>> {
        let dir = std::env::current_dir().context("can't read the current folder")?;
        game::find(&dir)
            .map(|path| game::load(&path, self.tools))
            .transpose()
    }

    /// The game around the current folder; `what` says what needs it ("forge sync").
    pub fn game(&self, what: &str) -> Result<Game> {
        self.game_here()?.ok_or_else(|| {
            anyhow!(
                "no {} here or above: {what} runs in a game's repo",
                game::FILE_NAME
            )
        })
    }

    /// The game repo the command runs in, so the server can match its place to a Studio.
    pub fn this_repo(&self) -> Result<Option<Repo>> {
        Ok(self.game_here()?.map(|g| Repo {
            root: g.root.display().to_string(),
            place: g.place,
        }))
    }
}

/// One MCP tool: a forge command line, or a few run one after another, built from the arguments it's called with.
/// Every MCP tool also takes `repo`, the game's folder to run in.
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub params: Vec<Param>,
    /// The forge command lines to run, from the arguments (checked against `params` first).
    pub commands: fn(&Map<String, Json>) -> Vec<Vec<String>>,
}

pub struct Param {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Flag,
    Count,
}

/// An argument as text, if it was given.
pub fn arg_text(args: &Map<String, Json>, name: &str) -> Option<String> {
    match args.get(name)? {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A flag argument: true only when given as true.
pub fn arg_flag(args: &Map<String, Json>, name: &str) -> bool {
    args.get(name).and_then(Json::as_bool) == Some(true)
}
