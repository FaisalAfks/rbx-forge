//! forge's MCP server (`forge mcp`, over stdio): sync's commands as tools for agents. Each tool runs the same `forge`
//! command a person would, in the game's repo, and returns what it printed, so the MCP and the command line can't drift
//! apart, and a command's own printing never mixes into the MCP's stdio.
//!
//! A push from here is proposed like any other: it applies only when Apply is clicked in Studio, unless the person in
//! Studio set forge's "Apply pushes" setting not to ask (decisions 0015, 0018). No tool can change that setting.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Result;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{
    ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router, transport::stdio,
};
use serde::Deserialize;

/// Runs forge commands in a game's repo.
#[derive(Debug, Clone)]
pub struct Forge {
    /// The forge program to run (this one).
    exe: PathBuf,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Repo {
    #[schemars(
        description = "The game repo's folder (the one with forge.toml, or below it). Leave out to use the \
                              folder the MCP server was started in."
    )]
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Pull {
    pub repo: Option<String>,
    #[schemars(
        description = "Pull into a folder forge didn't make, leave files forge doesn't write, and write over \
                              uncommitted changes. Leave off unless the person asked for it."
    )]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Check {
    pub repo: Option<String>,
    #[schemars(
        description = "Only this tree: its Studio path (StarterGui.Hud) or its folder. Leave out for all."
    )]
    pub tree: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Pick {
    pub repo: Option<String>,
    #[schemars(description = "The clash's number, as the clashes tool lists them.")]
    pub number: u32,
    #[schemars(description = "studio or disk: the side whose value wins.")]
    pub side: String,
}

#[tool_router]
impl Forge {
    #[tool(
        description = "Whether forge serve runs, which Studios are linked and to which game repo, and for this \
                          repo: whether its Studio is linked, scripts with an uncommitted draft, and clashes a running \
                          watch holds."
    )]
    async fn status(&self, Parameters(Repo { repo }): Parameters<Repo>) -> Result<String, String> {
        let server = self.run(repo.clone(), &["status"]).await?;
        let sync = self.run(repo, &["sync", "status"]).await?;
        Ok(format!("{server}\n{sync}"))
    }

    #[tool(
        description = "Write each tree in the game's forge.toml from Studio to its folder (layout C). Only changed \
                          files are written; it stops, writing nothing, on a folder forge didn't make or on \
                          uncommitted changes it would overwrite. Lists properties it can't sync and scripts with \
                          drafts."
    )]
    async fn pull(
        &self,
        Parameters(Pull { repo, force }): Parameters<Pull>,
    ) -> Result<String, String> {
        let mut args = vec!["sync", "pull"];
        if force == Some(true) {
            args.push("--force");
        }
        self.run(repo, &args).await
    }

    #[tool(
        description = "What differs between Studio and the game's files, one line per change, going from disk to \
                          Studio. Changes nothing."
    )]
    async fn diff(&self, Parameters(Repo { repo }): Parameters<Repo>) -> Result<String, String> {
        self.run(repo, &["sync", "diff"]).await
    }

    #[tool(
        description = "Propose making Studio match the files. Returns the plan and the result. Nothing changes until \
                          Apply is clicked in forge's window in Studio, unless the person in Studio set forge's Apply \
                          pushes setting not to ask; this tool can't apply on its own and can't change that setting. \
                          What a push replaces is backed up under .forge/backup first. Waits for the click (up to \
                          sync.confirm_timeout_ms)."
    )]
    async fn push(&self, Parameters(Repo { repo }): Parameters<Repo>) -> Result<String, String> {
        self.run(repo, &["sync", "push"]).await
    }

    #[tool(
        description = "Round-trip check: build a copy of each tree in Studio from its files, read it back and \
                          compare value by value. Lists each difference; the copy is gone afterwards and nothing else \
                          changes."
    )]
    async fn check(
        &self,
        Parameters(Check { repo, tree }): Parameters<Check>,
    ) -> Result<String, String> {
        let mut args = vec!["sync", "check"];
        if let Some(t) = tree.as_deref() {
            args.push(t);
        }
        self.run(repo, &args).await
    }

    #[tool(
        description = "Clashes a running forge sync watch holds (both sides changed the same value), numbered, \
                          with both values. Same as the status tool's last lines."
    )]
    async fn clashes(&self, Parameters(Repo { repo }): Parameters<Repo>) -> Result<String, String> {
        self.run(repo, &["sync", "status"]).await
    }

    #[tool(
        description = "Settle a clash a running watch holds: keep Studio's value (side studio) or the files' (side \
                          disk). Only when the person said which side should win."
    )]
    async fn pick(
        &self,
        Parameters(Pick { repo, number, side }): Parameters<Pick>,
    ) -> Result<String, String> {
        let number = number.to_string();
        self.run(repo, &["sync", "pick", &number, &side]).await
    }
}

impl Forge {
    /// Runs `forge <args>` in the repo and returns what it printed. A failing command is an error with its output.
    async fn run(&self, repo: Option<String>, args: &[&str]) -> Result<String, String> {
        let mut command = tokio::process::Command::new(&self.exe);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = repo {
            command.current_dir(dir);
        }
        let out = command
            .output()
            .await
            .map_err(|e| format!("can't run forge: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() {
            Ok(text)
        } else {
            Err(text)
        }
    }
}

#[tool_handler]
impl ServerHandler for Forge {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.server_info = Implementation::new("forge", env!("CARGO_PKG_VERSION"));
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "forge syncs a Roblox game between Studio and its git repo. Run tools in the game's repo (the folder with              forge.toml): status first, then pull, diff, push, check. A push applies only after Apply is clicked in              Studio unless the person there chose otherwise; never assume it applied, read the result."
                .into(),
        );
        info
    }
}

/// Serves the tools over stdio until the client goes away. `exe` is the forge program the tools run.
pub async fn serve(exe: PathBuf) -> Result<()> {
    let service = Forge { exe }.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
