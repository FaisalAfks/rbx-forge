//! forge sync as a forge tool (decision 0019): its command, its settings and `forge.toml` key, and its MCP tools.

use std::path::PathBuf;

use anyhow::Result;
use clap::{FromArgMatches, Subcommand};
use forge_core::game::Game;
use forge_core::tool::{Kind, McpTool, Param, arg_flag, arg_text};
use forge_core::{Cx, Tool, block_on};

use crate::settings::{self, Settings};
use crate::{commands, trees, watch};

/// The sync tool, as the binary, `forge mcp` and the plugin list it.
pub static SYNC: SyncTool = SyncTool;

pub struct SyncTool;

#[derive(Subcommand)]
enum SyncCommand {
    /// Write each tree in forge.toml from the Studio that has the place open to its folder.
    Pull {
        /// Pull into a folder forge didn't make, leave files forge doesn't write, and write over uncommitted changes.
        #[arg(long)]
        force: bool,
    },
    /// Say whether the game's Studio is linked, and list scripts with an uncommitted draft.
    Status,
    /// List what differs between Studio and the files: instances added, removed and moved, changed values, and
    /// scripts with the lines changed.
    Diff,
    /// Build a copy of each tree in Studio from its files, read it back, and compare it with the original value by
    /// value. The copy is gone afterwards and nothing else changes.
    Check {
        /// Only this tree: its Studio path (StarterGui.Hud) or its folder.
        tree: Option<String>,
    },
    /// Keep Studio and the files in step both ways until Ctrl+C. A value both sides changed differently is held as a
    /// clash until a side is picked.
    Watch,
    /// Settle a clash a running watch holds: keep Studio's value or the files'.
    Pick {
        /// The clash's number, as `forge sync status` lists them.
        number: usize,
        /// studio or disk: the side whose value wins.
        side: String,
    },
    /// Make Studio match the files. The plan shows in Studio, and nothing changes until Apply is clicked there.
    Push {
        /// Push the trees from this folder (laid out like the game's, such as a backup) instead of the game's own.
        #[arg(long)]
        from: Option<PathBuf>,
    },
}

impl Tool for SyncTool {
    fn name(&self) -> &'static str {
        "sync"
    }

    fn command(&self) -> clap::Command {
        let command = clap::Command::new("sync")
            .about("Sync a game's trees between Studio and its repo (forge.toml names them)")
            .subcommand_required(true)
            .arg_required_else_help(true);
        SyncCommand::augment_subcommands(command)
    }

    fn run(&self, cx: &Cx, matches: &clap::ArgMatches) -> Result<()> {
        let command = SyncCommand::from_arg_matches(matches)?;
        if let SyncCommand::Pick { number, side } = &command {
            return watch::pick(*number, side);
        }
        let s = Settings::from(cx.settings()?)?;
        block_on(async {
            match command {
                SyncCommand::Pull { force } => commands::pull(cx, &s, force).await,
                SyncCommand::Status => commands::status(cx, &s).await,
                SyncCommand::Diff => commands::diff(cx, &s).await,
                SyncCommand::Check { tree } => commands::check(cx, &s, tree.as_deref()).await,
                SyncCommand::Watch => watch::watch(cx, &s).await,
                SyncCommand::Push { from } => commands::push(cx, &s, from.as_deref()).await,
                SyncCommand::Pick { .. } => unreachable!("handled above"),
            }
        })
    }

    fn settings_defaults(&self) -> &'static str {
        settings::DEFAULTS
    }

    fn settings_toml(&self, s: &forge_core::settings::Settings) -> Result<String> {
        let sections: settings::Sections = s.sections(settings::SECTIONS)?;
        Ok(toml::to_string(&sections)?)
    }

    fn game_keys(&self) -> &'static [&'static str] {
        &[trees::KEY]
    }

    fn game_lines(&self, game: &Game) -> Result<Vec<String>> {
        Ok(trees::read(game)?
            .trees
            .iter()
            .map(|t| {
                format!(
                    "{} -> {}{}",
                    t.studio.join("."),
                    t.disk.display(),
                    t.kind.note()
                )
            })
            .collect())
    }

    fn mcp_tools(&self) -> Vec<McpTool> {
        mcp_tools()
    }
}

fn mcp_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "status",
            description: "Whether forge serve runs, which Studios are linked and to which game repo, and for this \
                          repo: whether its Studio is linked, scripts with an uncommitted draft, and clashes a running \
                          watch holds.",
            params: vec![],
            commands: |_| vec![line(&["status"]), line(&["sync", "status"])],
        },
        McpTool {
            name: "pull",
            description: "Write each tree in the game's forge.toml from Studio to its folder (layout C). Only changed \
                          files are written; it stops, writing nothing, on a folder forge didn't make or on \
                          uncommitted changes it would overwrite. Lists properties it can't sync and scripts with \
                          drafts.",
            params: vec![Param {
                name: "force",
                description: "Pull into a folder forge didn't make, leave files forge doesn't write, and write over \
                              uncommitted changes. Leave off unless the person asked for it.",
                kind: Kind::Flag,
                required: false,
            }],
            commands: |args| {
                let mut l = line(&["sync", "pull"]);
                if arg_flag(args, "force") {
                    l.push("--force".into());
                }
                vec![l]
            },
        },
        McpTool {
            name: "diff",
            description: "What differs between Studio and the game's files, one line per change, going from disk to \
                          Studio. Changes nothing.",
            params: vec![],
            commands: |_| vec![line(&["sync", "diff"])],
        },
        McpTool {
            name: "push",
            description: "Propose making Studio match the files. Returns the plan and the result. Nothing changes until \
                          Apply is clicked in forge's window in Studio, unless the person in Studio set forge's Apply \
                          pushes setting not to ask; this tool can't apply on its own and can't change that setting. \
                          What a push replaces is backed up under .forge/backup first. Waits for the click (up to \
                          sync.confirm_timeout_ms).",
            params: vec![],
            commands: |_| vec![line(&["sync", "push"])],
        },
        McpTool {
            name: "check",
            description: "Round-trip check: build a copy of each tree in Studio from its files, read it back and \
                          compare value by value. Lists each difference; the copy is gone afterwards and nothing else \
                          changes.",
            params: vec![Param {
                name: "tree",
                description: "Only this tree: its Studio path (StarterGui.Hud) or its folder. Leave out for all.",
                kind: Kind::Text,
                required: false,
            }],
            commands: |args| {
                let mut l = line(&["sync", "check"]);
                l.extend(arg_text(args, "tree"));
                vec![l]
            },
        },
        McpTool {
            name: "clashes",
            description: "Clashes a running forge sync watch holds (both sides changed the same value), numbered, \
                          with both values. Same as the status tool's last lines.",
            params: vec![],
            commands: |_| vec![line(&["sync", "status"])],
        },
        McpTool {
            name: "pick",
            description: "Settle a clash a running watch holds: keep Studio's value (side studio) or the files' (side \
                          disk). Only when the person said which side should win.",
            params: vec![
                Param {
                    name: "number",
                    description: "The clash's number, as the clashes tool lists them.",
                    kind: Kind::Count,
                    required: true,
                },
                Param {
                    name: "side",
                    description: "studio or disk: the side whose value wins.",
                    kind: Kind::Text,
                    required: true,
                },
            ],
            commands: |args| {
                let mut l = line(&["sync", "pick"]);
                l.extend(arg_text(args, "number"));
                l.extend(arg_text(args, "side"));
                vec![l]
            },
        },
    ]
}

fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}
