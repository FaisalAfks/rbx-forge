//! The `forge` command: one binary, a subcommand per tool. It reads forge's settings and a game's `forge.toml`, runs
//! the server that holds the link to Studio, installs forge's plugin, and pulls a game's trees to disk; the rest of
//! sync's commands join it step by step (docs/plan.md).

/// Prints a line to standard output. When nothing reads it any more (`forge sync diff | head`), the command ends there
/// instead of failing, as command-line tools do.
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if writeln!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

mod client;
mod game;
mod pack;
mod protocol;
mod server;
mod settings;
mod sync;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::protocol::{Message, Repo, StatusReport};
use crate::settings::Settings;

#[derive(Parser)]
#[command(
    name = "forge",
    version,
    about = "Roblox tooling: a whole game in git as readable files"
)]
struct Cli {
    /// Use this settings file instead of the usual one (`forge config` names it).
    #[arg(long, global = true)]
    settings: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show forge's settings, where they come from, and the game's forge.toml if there is one here.
    Config {
        /// The folder to look for forge.toml from (the nearest one in it or above it is used).
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Run the server that forge's plugin and commands connect to, in this window. Commands start it in the
    /// background when it isn't running, so this is only needed to watch its log.
    Serve {
        /// Write the log to this file instead of the terminal.
        #[arg(long)]
        log: Option<PathBuf>,
    },
    /// Stop the running server. Plugins keep trying, and link up again when it next starts.
    Stop,
    /// Show the server, the Studios connected to it, and the game repo each one belongs to.
    Status,
    /// Sync a game's trees between Studio and its repo (forge.toml names them).
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// forge's Studio plugin.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

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
    /// Make Studio match the files. The plan shows in Studio, and nothing changes until Apply is clicked there.
    Push {
        /// Push the trees from this folder (laid out like the game's, such as a backup) instead of the game's own.
        #[arg(long)]
        from: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Pack the plugin into forge.rbxmx and put it in Studio's Plugins folder. Studio loads it when it next starts.
    Install {
        /// Write the file here instead of Studio's Plugins folder.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print a Rojo-style sourcemap of the plugin, so luau-lsp can follow its requires.
    Sourcemap,
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        // `{:#}` prints the whole chain ("forge.toml is wrong: unknown field ..."), which is what tells people the fix.
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let file = cli.settings.or_else(settings::user_file);
    let file = file.as_deref();
    let settings = || settings::load(file);
    match cli.command {
        Command::Config { dir } => show_config(file, &dir),
        Command::Serve { log } => {
            let s = settings()?.server;
            let config = server::Config {
                port: s.port,
                request_timeout: std::time::Duration::from_millis(s.request_timeout_ms),
                max_message_bytes: s.max_message_bytes(),
            };
            block_on(server::run(config, log.as_deref()))
        }
        Command::Sync { command } => match command {
            SyncCommand::Pull { force } => block_on(sync::pull(&settings()?, file, force)),
            SyncCommand::Status => block_on(sync::status(&settings()?, file)),
            SyncCommand::Diff => block_on(sync::diff(&settings()?, file)),
            SyncCommand::Check { tree } => {
                block_on(sync::check(&settings()?, file, tree.as_deref()))
            }
            SyncCommand::Push { from } => block_on(sync::push(&settings()?, file, from.as_deref())),
        },
        Command::Stop => block_on(stop(&settings()?, file)),
        Command::Status => block_on(status(&settings()?, file)),
        Command::Plugin { command } => match command {
            PluginCommand::Install { out } => install_plugin(&settings()?, out.as_deref()),
            PluginCommand::Sourcemap => {
                let root = pack::tree(pack::files())?;
                say!("{}", serde_json::to_string_pretty(&pack::sourcemap(&root))?);
                Ok(())
            }
        },
    }
}

/// The link is async; one thread is plenty for a handful of connections.
fn block_on<F: std::future::Future<Output = Result<()>>>(future: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("can't start forge's async runtime")?
        .block_on(future)
}

/// The game repo the command runs in, so the server can match its place to a Studio.
pub(crate) fn this_repo() -> Result<Option<Repo>> {
    let dir = std::env::current_dir().context("can't read the current folder")?;
    let Some(path) = game::find(&dir) else {
        return Ok(None);
    };
    let g = game::load(&path)?;
    Ok(Some(Repo {
        root: g.root.display().to_string(),
        place: g.place,
    }))
}

async fn stop(settings: &Settings, file: Option<&Path>) -> Result<()> {
    let Some(mut link) = client::connect(settings, file, None, false).await? else {
        say!("forge serve isn't running");
        return Ok(());
    };
    match link.request(&Message::Stop).await? {
        Message::Stopping => {}
        other => bail!("forge serve answered stop with {other:?}"),
    }
    // Wait until the port is free, so a command right after this one starts a new server instead of finding the
    // old one on its way out.
    let port = settings.server.port;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(settings.server.start_timeout_ms);
    while tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .is_ok()
    {
        if std::time::Instant::now() >= deadline {
            bail!("forge serve said it was stopping, but port {port} is still taken");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    say!("stopped forge serve on port {port}");
    Ok(())
}

async fn status(settings: &Settings, file: Option<&Path>) -> Result<()> {
    let mut link = client::connect(settings, file, this_repo()?, true)
        .await?
        .expect("connect starts the server when asked to");
    let mut report = ask_status(&mut link).await?;
    if link.started {
        // The plugin only finds a new server on its next try, so give it one try's time before saying none is there.
        let log = client::log_file(file).map_or_else(|| "none".into(), |p| p.display().to_string());
        say!("started forge serve in the background (log: {log})");
        let wait = std::time::Duration::from_millis(
            settings.plugin.retry_ms + settings.plugin.open_timeout_ms,
        );
        let deadline = std::time::Instant::now() + wait;
        while report.studios.is_empty() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            report = ask_status(&mut link).await?;
        }
    }
    say!("{}", status_text(&report).trim_end());
    Ok(())
}

async fn ask_status(link: &mut client::Link) -> Result<StatusReport> {
    match link.request(&Message::Status).await? {
        Message::StatusReport(report) => Ok(report),
        other => bail!("forge serve answered status with {other:?}"),
    }
}

/// What `forge status` prints: the server, then each Studio with its repo, then repos with no Studio open.
fn status_text(report: &StatusReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = writeln!(
        out,
        "forge serve {} on 127.0.0.1:{}",
        report.version, report.port
    );
    if report.studios.is_empty() {
        out.push_str(
            "no Studio connected: is Studio open, with forge's plugin installed? \
             (`forge plugin install`, then restart Studio)\n",
        );
    }
    for s in &report.studios {
        let _ = writeln!(
            out,
            "studio: place {} (game {}) {:?}, linked {} s",
            s.place, s.game, s.name, s.connected_secs
        );
        match report.repos.iter().find(|r| r.place == s.place) {
            _ if s.place == 0 => out.push_str("  repo: none (the place was never published)\n"),
            Some(r) => {
                let _ = writeln!(out, "  repo: {}", r.root);
            }
            None => {
                let _ = writeln!(
                    out,
                    "  repo: none known (run forge in the repo whose forge.toml has place = {})",
                    s.place
                );
            }
        }
        if s.plugin != report.version {
            let _ = writeln!(
                out,
                "  plugin: from forge {}, not {} (run `forge plugin install`, then restart Studio)",
                s.plugin, report.version
            );
        }
    }
    for r in &report.refused {
        let _ = writeln!(out, "refused: a Studio on place {}: {}", r.place, r.message);
    }
    for r in &report.repos {
        if !report.studios.iter().any(|s| s.place == r.place) {
            let _ = writeln!(
                out,
                "repo with no Studio open: {} (place {})",
                r.root, r.place
            );
        }
    }
    out
}

fn install_plugin(settings: &Settings, out: Option<&Path>) -> Result<()> {
    let (path, scripts) = pack::install(settings, out)?;
    say!("wrote {} ({scripts} scripts)", path.display());
    if out.is_none() {
        say!("Studio loads plugin files only when it starts: restart Studio to load this one");
    }
    Ok(())
}

fn show_config(file: Option<&Path>, dir: &Path) -> Result<()> {
    let s = settings::load(file)?;
    match file {
        Some(path) if path.is_file() => say!("settings: {}", path.display()),
        Some(path) => say!("settings: defaults ({} doesn't exist)", path.display()),
        None => say!("settings: defaults (this system has no settings folder)"),
    }
    say!(
        "{}",
        toml::to_string(&s)
            .context("can't show the settings")?
            .trim_end()
    );

    let dir = std::path::absolute(dir).with_context(|| format!("can't use {}", dir.display()))?;
    match game::find(&dir) {
        None => say!(
            "\ngame: no {} in {} or above it",
            game::FILE_NAME,
            dir.display()
        ),
        Some(path) => {
            let g = game::load(&path)?;
            say!("\ngame: {}", path.display());
            say!("place = {}", g.place);
            for t in &g.trees {
                say!("{} -> {}", t.studio.join("."), t.disk.display());
            }
        }
    }
    Ok(())
}
