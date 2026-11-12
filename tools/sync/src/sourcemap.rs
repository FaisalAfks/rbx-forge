//! `forge sync sourcemap` (#79): the game's trees as a `sourcemap.json` in Rojo's format, so luau-lsp and selene
//! resolve `require`s and instance paths. Built from the files alone (`layout::outline`, `packages::read_with_files`),
//! so it runs without Studio, in CI too.
//!
//! Each tree sits under its Studio path. A service's class is its name; an instance between a service and a tree's
//! top is a Folder, except the containers Roblox makes itself (`StarterPlayer.StarterPlayerScripts`, ...). Like Rojo's,
//! the map holds only scripts and what leads to them unless non-scripts are asked for.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::layout;
use crate::packages;
use crate::tree::Node;
use crate::trees::{Game, Kind, Tree};

/// Instances Roblox puts under a service itself, whose class is their name.
const CONTAINERS: [(&str, &str); 2] = [
    ("StarterPlayer", "StarterPlayerScripts"),
    ("StarterPlayer", "StarterCharacterScripts"),
];

/// One instance in the map, as luau-lsp reads it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    #[serde(rename = "className")]
    pub class: String,
    #[serde(rename = "filePaths", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Entry>,
}

impl Entry {
    fn new(name: &str, class: &str) -> Entry {
        Entry {
            name: name.to_string(),
            class: class.to_string(),
            files: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The child named `name`, made with `class` if it isn't there yet.
    fn child(&mut self, name: &str, class: &str) -> &mut Entry {
        let i = match self.children.iter().position(|c| c.name == name) {
            Some(i) => i,
            None => {
                self.children.push(Entry::new(name, class));
                self.children.len() - 1
            }
        };
        &mut self.children[i]
    }

    /// Drops what holds no script, so the map has scripts and the instances that lead to them.
    fn keep_scripts(&mut self) -> bool {
        self.children.retain_mut(Entry::keep_scripts);
        !self.files.is_empty() || !self.children.is_empty()
    }

    /// Siblings by name, so the same files always give the same map.
    fn sort(&mut self) {
        self.children.sort_by(|a, b| a.name.cmp(&b.name));
        self.children.iter_mut().for_each(Entry::sort);
    }
}

/// The game's map: a DataModel named after the game's folder, with every tree under its Studio path.
pub fn build(game: &Game, non_scripts: bool) -> Result<Entry> {
    let name = game
        .root
        .file_name()
        .map_or("game".into(), |n| n.to_string_lossy().into_owned());
    let mut root = Entry::new(&name, "DataModel");
    root.files.push(forge_core::game::FILE_NAME.to_string());
    for tree in &game.trees {
        let top = tree_entry(&game.root, tree)
            .with_context(|| format!("can't map {}", tree.studio.join(".")))?;
        let (last, path) = tree.studio.split_last().expect("checked in trees::read");
        let mut at = &mut root;
        for (depth, name) in path.iter().enumerate() {
            let named_class =
                depth == 0 || CONTAINERS.contains(&(path[depth - 1].as_str(), name.as_str()));
            let class = if named_class { name.as_str() } else { "Folder" };
            at = at.child(name, class);
        }
        if at.children.iter().any(|c| c.name == *last) {
            bail!("two trees are both {}", tree.studio.join("."));
        }
        at.children.push(top);
    }
    if !non_scripts {
        root.children.retain_mut(Entry::keep_scripts);
    }
    root.sort();
    Ok(root)
}

/// One tree read from its folder, with file paths relative to the game's root.
fn tree_entry(root: &Path, tree: &Tree) -> Result<Entry> {
    let folder = root.join(&tree.disk);
    if let Some(why) = crate::commands::missing(tree.kind, &folder) {
        bail!("{}: {why}", tree.disk.display());
    }
    let disk = slashes(&tree.disk);
    let name = tree.studio.last().expect("checked in trees::read");
    match tree.kind {
        Kind::Synced => {
            let files = layout::read_dir(&folder, &tree.ignore)?;
            let node =
                layout::outline(&files).with_context(|| format!("reading {}", folder.display()))?;
            let mut top = from_layout(&node, None, "", &disk);
            top.name = name.clone();
            Ok(top)
        }
        Kind::Packages => {
            let (node, files) = packages::read_with_files(&folder, name)?;
            let path_of = |id: u32| Some(format!("{disk}/{}", slashes(files.get(&id)?)));
            Ok(from_packages(&node, &path_of))
        }
    }
}

/// A layout C node. `dir` is the folder it would have; `parent` its parent's folder, or none for the tree's top,
/// whose text is always its folder's init file.
fn from_layout(node: &Node, parent: Option<&str>, dir: &str, disk: &str) -> Entry {
    let mut e = Entry::new(&node.name, &node.class);
    let key = node.key.as_deref().unwrap_or("");
    let path = match parent {
        Some(parent) if node.children.is_empty() => layout::script_path(node, parent, key),
        _ => layout::init_path(node, dir),
    };
    if let Some(path) = path {
        e.files.push(format!("{disk}/{path}"));
    }
    for child in &node.children {
        let child_dir = join(dir, child.key.as_deref().unwrap_or(""));
        e.children
            .push(from_layout(child, Some(dir), &child_dir, disk));
    }
    e
}

fn from_packages(node: &Node, path_of: &dyn Fn(u32) -> Option<String>) -> Entry {
    let mut e = Entry::new(&node.name, &node.class);
    e.files.extend(path_of(node.id));
    e.children = node
        .children
        .iter()
        .map(|c| from_packages(c, path_of))
        .collect();
    e
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// A path with `/` between its parts, as sourcemaps write them on every system.
fn slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Writes the map as JSON.
pub fn write(entry: &Entry, to: &Path) -> Result<()> {
    let text = serde_json::to_string(entry)?;
    std::fs::write(to, text).with_context(|| format!("can't write {}", to.display()))
}
