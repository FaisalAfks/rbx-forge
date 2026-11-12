//! A game's `forge.toml`: which place it is, plus each tool's own keys (sync's `[[sync]]` trees, for one). forge itself
//! never knows about one game; everything game-specific is in this file, in the game's repo.
//!
//! Core reads `place` and checks that every other key belongs to a tool. Each tool reads and checks its own keys
//! (`Tool::game_lines`), so a game file with a mistake in it is refused by every command, whichever tool it's for.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;

use crate::tool::Tool;

pub const FILE_NAME: &str = "forge.toml";

/// A game's `forge.toml`: its place, and the rest of the file for tools to read.
#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    /// The folder that holds `forge.toml`; every disk path is relative to it.
    pub root: PathBuf,
    pub place: u64,
    /// The keys other than `place`, which belong to tools.
    rest: toml::Table,
}

impl Game {
    /// A tool's key, read as `T`; missing means `T::default()`.
    pub fn get<T: DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        match self.rest.get(key) {
            None => Ok(T::default()),
            Some(value) => value
                .clone()
                .try_into()
                .with_context(|| format!("`{key}` is wrong")),
        }
    }
}

/// Finds `forge.toml` in `start` or the nearest folder above it, the way git finds its repo.
pub fn find(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|p| p.is_file())
}

/// Reads a `forge.toml` and has every tool check its keys.
pub fn load(path: &Path, tools: &[&dyn Tool]) -> Result<Game> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("can't read {}", path.display()))?;
    let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    parse(&text, root, tools).with_context(|| format!("{} is wrong", path.display()))
}

/// Parses the text of a `forge.toml` whose folder is `root`, and has every tool check its keys.
pub fn parse(text: &str, root: PathBuf, tools: &[&dyn Tool]) -> Result<Game> {
    let mut rest: toml::Table = text.parse()?;
    let place = match rest.remove("place") {
        None => bail!("missing `place`: the place id from Studio"),
        Some(toml::Value::Integer(id)) if id > 0 => id as u64,
        Some(_) => bail!("`place` must be the place id from Studio (a number above 0)"),
    };
    let known: Vec<&str> = std::iter::once("place")
        .chain(tools.iter().flat_map(|t| t.game_keys().iter().copied()))
        .collect();
    if let Some(key) = rest.keys().find(|k| !known.contains(&k.as_str())) {
        bail!("unknown key `{key}`, expected one of {}", quoted(&known));
    }
    let game = Game { root, place, rest };
    for tool in tools {
        tool.game_lines(&game)?;
    }
    Ok(game)
}

/// `a`, `b`, `c`: names as an error message lists them.
pub(crate) fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(text: &str) -> String {
        format!("{:#}", parse(text, PathBuf::from("repo"), &[]).unwrap_err())
    }

    #[test]
    fn reads_the_place() {
        let g = parse("place = 1234", PathBuf::from("repo"), &[]).unwrap();
        assert_eq!(g.place, 1234);
    }

    #[test]
    fn needs_a_place() {
        assert!(error("").contains("place"));
        assert!(error("place = 0").contains("place id"));
    }

    #[test]
    fn rejects_keys_no_tool_owns() {
        assert!(error("place = 1\nplaec = 2").contains("plaec"));
    }

    #[test]
    fn finds_the_file_in_a_parent_folder() {
        let root = std::env::temp_dir().join(format!("forge-find-{}", std::process::id()));
        let deep = root.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(root.join(FILE_NAME), "place = 1").unwrap();
        assert_eq!(find(&deep), Some(root.join(FILE_NAME)));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
