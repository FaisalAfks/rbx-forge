//! Putting a pulled tree's files on disk safely: only files whose bytes changed are written, only forge's own files
//! are removed, and a folder forge didn't make or a file with uncommitted changes in git stops the pull unless it's
//! forced. A `.gitattributes` in each synced folder keeps git from changing the bytes forge wrote.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::layout::{self, Files};

/// Tells git to keep every file in the folder byte for byte: no line-ending changes on checkout or commit, whatever
/// `core.autocrlf` or a parent `.gitattributes` says (docs/sync/format.md).
pub const GITATTRIBUTES: (&str, &str) = (
    ".gitattributes",
    "# forge writes these files byte for byte (docs/sync/format.md); git must not change their line endings.\n* -text\n",
);

/// What a pull does to one folder.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub write: Vec<String>,
    pub remove: Vec<String>,
    pub unchanged: usize,
}

/// Works out the changes that put `files` in `folder`, and checks they're safe. `force` skips the checks for a
/// folder forge didn't make and for uncommitted changes.
pub fn plan(folder: &Path, files: &Files, force: bool) -> Result<Plan> {
    let mut files = files.clone();
    files.insert(GITATTRIBUTES.0.into(), GITATTRIBUTES.1.as_bytes().to_vec());
    let old = if folder.is_dir() {
        layout::read_dir(folder)?
    } else {
        Files::new()
    };

    if !force && !old.is_empty() && !old.contains_key("_.toml") {
        bail!(
            "{} has files but no _.toml, so forge didn't make it. Pull into an empty folder, or pass --force to \
             write over it",
            folder.display()
        );
    }
    let mut plan = Plan::default();
    for (rel, bytes) in &files {
        let path = folder.join(rel);
        match std::fs::read(&path) {
            Ok(existing) if &existing == bytes => plan.unchanged += 1,
            _ => plan.write.push(rel.clone()),
        }
    }
    for rel in old.keys().filter(|rel| !files.contains_key(*rel)) {
        if is_forge_file(rel) {
            plan.remove.push(rel.clone());
        } else if !force {
            bail!(
                "{}: forge doesn't write this file, so pull won't remove it. Move it out of the synced folder, or \
                 pass --force to leave it there",
                folder.join(rel).display()
            );
        }
    }
    if !force {
        let touched: Vec<&String> = plan.write.iter().chain(&plan.remove).collect();
        let dirty = uncommitted(folder, &touched)?;
        if !dirty.is_empty() {
            bail!(
                "pull would change files with uncommitted changes: {}. Commit or stash them, or pass --force",
                dirty.join(", ")
            );
        }
    }
    Ok(plan)
}

/// Carries out a plan made by `plan`.
pub fn apply(folder: &Path, files: &Files, plan: &Plan) -> Result<()> {
    for rel in &plan.write {
        let bytes = match files.get(rel) {
            Some(bytes) => bytes.as_slice(),
            None if rel == GITATTRIBUTES.0 => GITATTRIBUTES.1.as_bytes(),
            None => bail!("{rel} isn't among the pulled files"),
        };
        let path = folder.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("can't make {}", parent.display()))?;
        }
        std::fs::write(&path, bytes).with_context(|| format!("can't write {}", path.display()))?;
    }
    for rel in &plan.remove {
        let path = folder.join(rel);
        std::fs::remove_file(&path).with_context(|| format!("can't remove {}", path.display()))?;
        remove_empty_parents(folder, &path);
    }
    Ok(())
}

/// The files layout C writes. Only these are ever removed.
fn is_forge_file(rel: &str) -> bool {
    [".toml", ".luau", ".rbxm"]
        .iter()
        .any(|ext| rel.ends_with(ext))
}

/// Removes folders a removal left empty, up to (not including) the synced folder.
fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir: Option<PathBuf> = file.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d == root || std::fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

/// Which of `paths` (relative to `folder`) git has uncommitted changes for. Untracked files don't count: they're an
/// earlier pull nobody committed yet. Outside a git repo, or without git, nothing does.
fn uncommitted(folder: &Path, paths: &[&String]) -> Result<Vec<String>> {
    if paths.is_empty() || !folder.is_dir() {
        return Ok(vec![]);
    }
    let out = match Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["status", "--porcelain=v1", "-z", "--"])
        .args(paths)
        .output()
    {
        Ok(out) if out.status.success() => out,
        _ => return Ok(vec![]),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split('\0')
        .filter(|entry| entry.len() > 3 && !entry.starts_with("??"))
        .map(|entry| entry[3..].to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-disk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn files(list: &[(&str, &str)]) -> Files {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
            .collect()
    }

    fn pull(dir: &Path, f: &Files) -> Plan {
        let p = plan(dir, f, false).unwrap();
        apply(dir, f, &p).unwrap();
        p
    }

    #[test]
    fn a_second_pull_with_nothing_changed_writes_nothing() {
        let dir = temp("same");
        let f = files(&[
            ("_.toml", "class = \"Folder\"\n"),
            ("A/_.toml", "class = \"Folder\"\n"),
        ]);
        let first = pull(&dir, &f);
        assert_eq!(first.write.len(), 3, "two files and .gitattributes");
        let second = pull(&dir, &f);
        assert!(
            second.write.is_empty() && second.remove.is_empty(),
            "{second:?}"
        );
        assert_eq!(second.unchanged, 3);
    }

    #[test]
    fn removed_instances_lose_their_files_and_empty_folders() {
        let dir = temp("remove");
        pull(
            &dir,
            &files(&[("_.toml", "x"), ("A/_.toml", "a"), ("A/S.luau", "print(1)")]),
        );
        let p = pull(&dir, &files(&[("_.toml", "y")]));
        assert_eq!(p.write, ["_.toml"]);
        assert_eq!(p.remove, ["A/S.luau", "A/_.toml"]);
        assert!(!dir.join("A").exists());
        assert!(dir.join(".gitattributes").exists());
    }

    #[test]
    fn a_folder_forge_didnt_make_is_left_alone() {
        let dir = temp("foreign");
        std::fs::write(dir.join("init.luau"), "return {}").unwrap();
        let err = plan(&dir, &files(&[("_.toml", "x")]), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no _.toml"), "{err}");
        // Forced, pull writes over it, and a file shaped like one of its own goes.
        let p = plan(&dir, &files(&[("_.toml", "x")]), true).unwrap();
        assert!(
            p.remove.contains(&"init.luau".to_string()),
            "forge-shaped files are replaced when forced"
        );
    }

    #[test]
    fn a_file_forge_doesnt_write_stops_the_pull() {
        let dir = temp("stray");
        pull(&dir, &files(&[("_.toml", "x")]));
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();
        let err = plan(&dir, &files(&[("_.toml", "x")]), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("notes.txt"), "{err}");
        assert!(
            plan(&dir, &files(&[("_.toml", "x")]), true)
                .unwrap()
                .remove
                .is_empty()
        );
    }

    #[test]
    fn uncommitted_changes_stop_the_pull() {
        let dir = temp("git");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .map(|o| o.status.success());
            ok.unwrap_or(false)
        };
        if !git(&["init", "-q"]) {
            return; // no git on this system: the guard can't apply
        }
        pull(&dir, &files(&[("_.toml", "x")]));
        assert!(git(&["add", "."]));
        assert!(git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "pull"
        ]));
        std::fs::write(dir.join("_.toml"), "edited by hand").unwrap();
        let err = plan(&dir, &files(&[("_.toml", "from studio")]), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("_.toml"), "{err}");
        assert!(plan(&dir, &files(&[("_.toml", "from studio")]), true).is_ok());
    }
}
