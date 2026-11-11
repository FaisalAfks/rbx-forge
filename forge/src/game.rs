//! A game's `forge.toml`: which place it is, and which parts of the place are synced to which folders. forge itself
//! never knows about one game; everything game-specific is in this file, in the game's repo.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const FILE_NAME: &str = "forge.toml";

/// A game's settings, read from its `forge.toml`.
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    place: u64,
    #[serde(default)]
    sync: Vec<FileTree>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTree {
    studio: StudioPath,
    disk: String,
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

/// Finds `forge.toml` in `start` or the nearest folder above it, the way git finds its repo.
pub fn find(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|p| p.is_file())
}

/// Reads and checks a `forge.toml`.
pub fn load(path: &Path) -> Result<Game> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("can't read {}", path.display()))?;
    let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    parse(&text, root).with_context(|| format!("{} is wrong", path.display()))
}

/// Parses the text of a `forge.toml` whose folder is `root`.
pub fn parse(text: &str, root: PathBuf) -> Result<Game> {
    let file: File = toml::from_str(text)?;
    if file.place == 0 {
        bail!("`place` must be the place id from Studio (a number above 0)");
    }
    let trees = file
        .sync
        .into_iter()
        .map(|t| check_tree(t.studio.names(), &t.disk))
        .collect::<Result<Vec<_>>>()?;
    check_no_overlap(&trees)?;
    Ok(Game {
        root,
        place: file.place,
        trees,
    })
}

fn check_tree(studio: Vec<String>, disk: &str) -> Result<Tree> {
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
        parse(text, PathBuf::from("repo"))
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
    fn no_trees_is_allowed() {
        assert!(game("place = 1").unwrap().trees.is_empty());
    }

    #[test]
    fn needs_a_place() {
        assert!(error("").contains("place"));
        assert!(error("place = 0").contains("place id"));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(error("place = 1\nplaec = 2").contains("plaec"));
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
