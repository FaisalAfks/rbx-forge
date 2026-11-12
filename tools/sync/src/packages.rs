//! Packages trees (decision 0021): a folder that `wally install` or `pesde install` made, read the way Rojo reads it,
//! so forge puts the same instances in Studio that Rojo did. Disk owns these trees: forge reads them and pushes, and
//! never writes into them.
//!
//! Only the part of Rojo's format that packages use is read (Rojo 7.7's rules, `docs/sync/packages.md`):
//! - a folder is a Folder, unless it holds an init file (`init.luau`, `init.lua`, then the `.server` and `.client`
//!   ones), which makes it that script;
//! - `Name.lua` and `Name.luau` are ModuleScripts, `.server` ones Scripts and `.client` ones LocalScripts;
//! - a folder with a `default.project.json` is only what that project says: its `tree` may hold `$path` alone, and the
//!   instance is named by the project's `name`, or by the folder when it has none.
//!
//! A file Rojo would turn into another kind of instance (JSON, TOML, text, models, meta files) is an error naming it,
//! so a package forge can't read whole never lands in Studio different from Rojo's. Files Rojo skips are skipped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::tree::Node;

/// The project file that stands in for a folder's contents.
const PROJECT_FILE: &str = "default.project.json";

/// A folder's init files, in the order Rojo looks for them, and the class each makes the folder.
const INIT_FILES: [(&str, &str); 6] = [
    ("init.luau", "ModuleScript"),
    ("init.lua", "ModuleScript"),
    ("init.server.luau", "Script"),
    ("init.server.lua", "Script"),
    ("init.client.luau", "LocalScript"),
    ("init.client.lua", "LocalScript"),
];

/// Script file endings and their classes, longest first so `.server.lua` isn't read as `.lua`.
const SCRIPT_ENDINGS: [(&str, &str); 6] = [
    (".server.luau", "Script"),
    (".server.lua", "Script"),
    (".client.luau", "LocalScript"),
    (".client.lua", "LocalScript"),
    (".luau", "ModuleScript"),
    (".lua", "ModuleScript"),
];

/// File endings Rojo makes some other instance from. forge refuses them rather than leave them out.
const UNREAD_ENDINGS: [&str; 16] = [
    ".plugin.luau",
    ".plugin.lua",
    ".project.json",
    ".project.jsonc",
    ".model.json",
    ".model.jsonc",
    ".meta.json",
    ".meta.jsonc",
    ".json",
    ".jsonc",
    ".toml",
    ".csv",
    ".txt",
    ".rbxm",
    ".rbxmx",
    ".yaml",
];

/// Top-level keys of a project file that don't change what's in the tree.
const PROJECT_KEYS: [&str; 7] = [
    "name",
    "tree",
    "servePort",
    "servePlaceIds",
    "serveAddress",
    "placeId",
    "gameId",
];

/// Reads a packages folder as the tree named `name` (the last name of its Studio path).
pub fn read(folder: &Path, name: &str) -> Result<Node> {
    if !folder.is_dir() {
        bail!(
            "{} isn't a folder: run `wally install` (or `pesde install`) first",
            folder.display()
        );
    }
    let mut reader = Reader {
        root: folder.to_path_buf(),
        ids: 0,
        projects: BTreeSet::new(),
    };
    let mut node = reader.folder(folder, name)?;
    node.name = name.to_string();
    Ok(node)
}

struct Reader {
    /// The tree's folder: a project's `$path` can't leave it.
    root: PathBuf,
    ids: u32,
    /// Project files being read, so a `$path` that leads back to its own project is an error, not a loop.
    projects: BTreeSet<PathBuf>,
}

impl Reader {
    fn node(&mut self, class: &str, name: &str) -> Node {
        self.ids += 1;
        Node {
            id: self.ids,
            class: class.to_string(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// A folder: its project if it has one, else a Folder or the script its init file makes, with its contents.
    fn folder(&mut self, dir: &Path, name: &str) -> Result<Node> {
        let project = dir.join(PROJECT_FILE);
        if project.is_file() {
            return self.project(&project, name);
        }
        let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
            .with_context(|| format!("can't read {}", dir.display()))?
            .map(|e| {
                let e = e.with_context(|| format!("can't read {}", dir.display()))?;
                Ok((e.file_name().to_string_lossy().into_owned(), e.path()))
            })
            .collect::<Result<_>>()?;
        entries.sort();

        let inits: Vec<&(&str, &str)> = INIT_FILES
            .iter()
            .filter(|(file, _)| dir.join(file).is_file())
            .collect();
        if inits.len() > 1 {
            let files: Vec<&str> = inits.iter().map(|(f, _)| *f).collect();
            bail!(
                "{} has {} init files ({}): a folder can be one script only",
                dir.display(),
                inits.len(),
                files.join(", ")
            );
        }
        let mut node = match inits.first() {
            Some((file, class)) => {
                let mut n = self.node(class, name);
                n.source = Some(text(&dir.join(file))?);
                n
            }
            None => self.node("Folder", name),
        };
        for (file_name, path) in entries {
            if path.is_dir() {
                node.children.push(self.folder(&path, &file_name)?);
            } else if INIT_FILES.iter().all(|(f, _)| *f != file_name)
                && let Some(child) = self.file(&path, &file_name)?
            {
                node.children.push(child);
            }
        }
        Ok(node)
    }

    /// A file: the script its name says, or nothing for a file Rojo skips.
    fn file(&mut self, path: &Path, file_name: &str) -> Result<Option<Node>> {
        if let Some(ending) = UNREAD_ENDINGS.iter().find(|e| file_name.ends_with(*e)) {
            bail!(
                "{}: Rojo makes an instance from `{ending}` files, and forge reads only .lua and .luau scripts in \
                 packages",
                path.display()
            );
        }
        let Some((ending, class)) = SCRIPT_ENDINGS.iter().find(|(e, _)| {
            file_name
                .strip_suffix(e)
                .is_some_and(|stem| !stem.is_empty())
        }) else {
            return Ok(None);
        };
        let mut node = self.node(class, &file_name[..file_name.len() - ending.len()]);
        node.source = Some(text(path)?);
        Ok(Some(node))
    }

    /// A `default.project.json`: the instance its `tree` points at, named by the project's `name` or the folder's.
    fn project(&mut self, file: &Path, folder_name: &str) -> Result<Node> {
        let what = || file.display().to_string();
        let key = std::path::absolute(file).with_context(what)?;
        if !self.projects.insert(key.clone()) {
            bail!("{}: its `$path` leads back to itself", what());
        }
        let body =
            std::fs::read_to_string(file).with_context(|| format!("can't read {}", what()))?;
        let project: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&body).with_context(|| format!("{} isn't JSON", what()))?;
        if let Some(k) = project.keys().find(|k| !PROJECT_KEYS.contains(&k.as_str())) {
            bail!(
                "{}: forge doesn't read `{k}` in a package's project file (only `name` and `tree` with `$path`)",
                what()
            );
        }
        let tree: Tree = serde_json::from_value(
            project
                .get("tree")
                .cloned()
                .ok_or_else(|| anyhow!("{} has no `tree`", what()))?,
        )
        .map_err(|e| anyhow!("{}: a package's `tree` can hold only `$path` ({e})", what()))?;
        let name = match project.get("name") {
            Some(n) => n
                .as_str()
                .ok_or_else(|| anyhow!("{}: `name` must be text", what()))?
                .to_string(),
            None => folder_name.to_string(),
        };

        let dir = file.parent().unwrap_or(Path::new("."));
        let target = std::path::absolute(dir.join(&tree.path)).with_context(what)?;
        let root = std::path::absolute(&self.root).with_context(what)?;
        if !normal(&target).starts_with(normal(&root)) {
            bail!(
                "{}: `$path` {:?} leads out of the packages folder",
                what(),
                tree.path
            );
        }
        let mut node = if target.is_dir() {
            self.folder(&target, &name)?
        } else if target.is_file() {
            let file_name = target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.file(&target, &file_name)?.ok_or_else(|| {
                anyhow!(
                    "{}: `$path` {:?} is a file Rojo doesn't read",
                    what(),
                    tree.path
                )
            })?
        } else {
            bail!("{}: `$path` {:?} isn't there", what(), tree.path);
        };
        node.name = name;
        self.projects.remove(&key);
        Ok(node)
    }
}

/// A package project's `tree`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tree {
    #[serde(rename = "$path")]
    path: String,
}

/// A path with `.` and `..` worked out, so a `$path` can be checked against the packages folder.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// A script's text, which has to be UTF-8.
fn text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| anyhow!("{} isn't UTF-8 text", path.display()))
}
