//! sync's part of a game's `forge.toml`: the `[[sync]]` trees, each a part of the place and the folder it's synced to.
//! Core reads the rest of the file (`forge_core::game`).

use std::path::{Component, PathBuf};

use anyhow::{Result, bail};
use forge_core::game::Game as CoreGame;
use serde::Deserialize;

/// The `forge.toml` key sync owns.
pub const KEY: &str = "sync";

/// A game as sync sees it: its folder, its place and its trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    /// The folder that holds `forge.toml`; every disk path is relative to it.
    pub root: PathBuf,
    pub place: u64,
    pub trees: Vec<Tree>,
}

/// One part of the place and the folder it's synced to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    /// The instance's path in Studio, one name per step, starting with a service (`["StarterGui", "Hud"]`).
    pub studio: Vec<String>,
    /// The folder on disk, relative to the game's root.
    pub disk: PathBuf,
    pub kind: Kind,
}

/// Who owns a tree's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Both sides: pull writes the files in layout C, push and watch carry the files to Studio.
    #[default]
    Synced,
    /// Disk alone (`kind = "packages"`, decision 0021): files `wally install` or `pesde install` made, read the way
    /// Rojo reads them. forge pushes them and never writes into the folder.
    Packages,
}

impl Kind {
    /// How `forge config` and the commands name it, after the tree.
    pub fn note(self) -> &'static str {
        match self {
            Kind::Synced => "",
            Kind::Packages => " (packages: disk owns it)",
        }
    }
}

/// The values `kind` can take in forge.toml.
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FileKind {
    Packages,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTree {
    studio: StudioPath,
    disk: String,
    kind: Option<FileKind>,
}

/// Reads and checks a game's `[[sync]]` trees.
pub fn read(game: &CoreGame) -> Result<Game> {
    let trees = game
        .get::<Vec<FileTree>>(KEY)?
        .into_iter()
        .map(|t| {
            let kind = match t.kind {
                None => Kind::Synced,
                Some(FileKind::Packages) => Kind::Packages,
            };
            check_tree(t.studio.names(), &t.disk, kind)
        })
        .collect::<Result<Vec<_>>>()?;
    check_no_overlap(&trees)?;
    Ok(Game {
        root: game.root.clone(),
        place: game.place,
        trees,
    })
}

/// A Studio path is written `"StarterGui.Hud"`, or as a list of names when a name has a dot in it.
#[derive(Deserialize)]
#[serde(untagged)]
enum StudioPath {
    Dotted(String),
    Names(Vec<String>),
}

impl StudioPath {
    fn names(self) -> Vec<String> {
        match self {
            StudioPath::Dotted(s) => s.split('.').map(str::to_string).collect(),
            StudioPath::Names(names) => names,
        }
    }
}

fn check_tree(studio: Vec<String>, disk: &str, kind: Kind) -> Result<Tree> {
    if studio.is_empty() || studio.iter().any(String::is_empty) {
        bail!("studio path {:?} has an empty name", studio.join("."));
    }
    if studio.len() < 2 {
        bail!(
            "studio path {:?} is a whole service; name a part of it, like \"{}.Hud\"",
            studio[0],
            studio[0]
        );
    }
    let disk_path = PathBuf::from(disk);
    let inside = disk_path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if disk.is_empty() || !inside {
        bail!("disk path {disk:?} must be a folder inside the game's repo, like \"game/ui\"");
    }
    Ok(Tree {
        studio,
        disk: disk_path,
        kind,
    })
}

/// Two trees can't share a part of the place or a folder, or the same file would be written from two places.
fn check_no_overlap(trees: &[Tree]) -> Result<()> {
    for (i, a) in trees.iter().enumerate() {
        for b in &trees[i + 1..] {
            if a.studio.starts_with(&b.studio) || b.studio.starts_with(&a.studio) {
                bail!(
                    "studio paths {:?} and {:?} overlap",
                    a.studio.join("."),
                    b.studio.join(".")
                );
            }
            if a.disk.starts_with(&b.disk) || b.disk.starts_with(&a.disk) {
                bail!("disk paths {:?} and {:?} overlap", a.disk, b.disk);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(text: &str) -> Result<Game> {
        let core = forge_core::game::parse(text, PathBuf::from("repo"), &[&crate::tool::SYNC])?;
        read(&core)
    }

    fn error(text: &str) -> String {
        format!("{:#}", game(text).unwrap_err())
    }

    #[test]
    fn reads_a_game() {
        let g = game(
            r#"
            place = 1234
            [[sync]]
            studio = "StarterGui.Hud"
            disk = "game/ui/hud"
            [[sync]]
            studio = ["ReplicatedStorage", "Shared.v2"]
            disk = "game/shared"
            "#,
        )
        .unwrap();
        assert_eq!(g.place, 1234);
        assert_eq!(g.trees[0].studio, ["StarterGui", "Hud"]);
        assert_eq!(g.trees[0].disk, PathBuf::from("game/ui/hud"));
        assert_eq!(g.trees[1].studio, ["ReplicatedStorage", "Shared.v2"]);
    }

    #[test]
    fn reads_a_packages_tree() {
        let g = game(
            "place = 1
[[sync]]
studio = \"ReplicatedStorage.Packages\"
disk = \"Packages\"
kind = \"packages\"
             [[sync]]
studio = \"StarterGui.Hud\"
disk = \"ui\"",
        )
        .unwrap();
        assert_eq!(g.trees[0].kind, Kind::Packages);
        assert_eq!(g.trees[1].kind, Kind::Synced);
        assert!(
            error(
                "place = 1
[[sync]]
studio = \"A.B\"
disk = \"a\"
kind = \"rojo\""
            )
            .contains("unknown variant")
        );
    }

    #[test]
    fn no_trees_is_allowed() {
        assert!(game("place = 1").unwrap().trees.is_empty());
    }

    #[test]
    fn rejects_bad_studio_paths() {
        assert!(
            error("place = 1\n[[sync]]\nstudio = \"StarterGui\"\ndisk = \"a\"")
                .contains("whole service")
        );
        assert!(
            error("place = 1\n[[sync]]\nstudio = \"StarterGui..Hud\"\ndisk = \"a\"")
                .contains("empty name")
        );
    }

    #[test]
    fn rejects_disk_paths_outside_the_repo() {
        for disk in ["../other", "/abs", ""] {
            let text = format!("place = 1\n[[sync]]\nstudio = \"StarterGui.Hud\"\ndisk = {disk:?}");
            assert!(error(&text).contains("inside the game's repo"), "{disk}");
        }
    }

    #[test]
    fn rejects_overlapping_trees() {
        let nested_studio = r#"
            place = 1
            [[sync]]
            studio = "StarterGui.Hud"
            disk = "a"
            [[sync]]
            studio = "StarterGui.Hud.Bar"
            disk = "b"
        "#;
        assert!(error(nested_studio).contains("overlap"));
        let same_disk = r#"
            place = 1
            [[sync]]
            studio = "StarterGui.Hud"
            disk = "game/ui"
            [[sync]]
            studio = "Workspace.Arena"
            disk = "game/ui/arena"
        "#;
        assert!(error(same_disk).contains("overlap"));
    }
}
