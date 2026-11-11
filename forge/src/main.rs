//! The `forge` command: one binary, a subcommand per tool. For now it reads forge's settings and a game's
//! `forge.toml`; sync's commands join it step by step (docs/plan.md).

mod game;
mod settings;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "forge",
    version,
    about = "Roblox tooling: a whole game in git as readable files"
)]
struct Cli {
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
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        // `{:#}` prints the whole chain ("forge.toml is wrong: unknown field ..."), which is what tells people the fix.
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Config { dir } => show_config(&dir),
    }
}

fn show_config(dir: &std::path::Path) -> Result<()> {
    let file = settings::user_file();
    let s = settings::load(file.as_deref())?;
    match &file {
        Some(path) if path.is_file() => println!("settings: {}", path.display()),
        Some(path) => println!("settings: defaults ({} doesn't exist)", path.display()),
        None => println!("settings: defaults (this system has no settings folder)"),
    }
    print!(
        "{}",
        toml::to_string(&s).context("can't show the settings")?
    );

    let dir = std::path::absolute(dir).with_context(|| format!("can't use {}", dir.display()))?;
    match game::find(&dir) {
        None => println!(
            "\ngame: no {} in {} or above it",
            game::FILE_NAME,
            dir.display()
        ),
        Some(path) => {
            let g = game::load(&path)?;
            println!("\ngame: {}", path.display());
            println!("place = {}", g.place);
            for t in &g.trees {
                println!("{} -> {}", t.studio.join("."), t.disk.display());
            }
        }
    }
    Ok(())
}
