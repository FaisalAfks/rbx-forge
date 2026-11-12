//! Files a tree leaves alone (`ignore` in a `[[sync]]`, #78): READMEs and other notes kept beside the code. A skipped
//! file is never read, written or removed. Patterns work like `.gitignore` lines: one without a `/` matches a file's
//! name at any depth (`*.md`), one with a `/` matches its whole path in the tree's folder (`docs/**`). `*` and `?`
//! stay within a name; `**` crosses folders.

use anyhow::{Result, bail};

/// A tree's ignore patterns, checked when `forge.toml` is read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ignore {
    patterns: Vec<String>,
}

/// Files forge writes or reads itself, which no pattern may hide: hiding one would make a push remove what it holds.
const OWN_FILES: [&str; 5] = [
    "_.toml",
    "Name.luau",
    "Name.server.luau",
    "Name.rbxm",
    ".gitattributes",
];

impl Ignore {
    pub fn new(patterns: Vec<String>) -> Result<Ignore> {
        for p in &patterns {
            if p.is_empty() || p.starts_with('/') || p.split('/').any(|part| part == "..") {
                bail!(
                    "ignore pattern {p:?} must be a file name or a path inside the tree's folder"
                );
            }
            let ignore = Ignore {
                patterns: vec![p.clone()],
            };
            if let Some(own) = OWN_FILES.iter().find(|f| ignore.skips(f)) {
                bail!("ignore pattern {p:?} would hide forge's own files (like {own})");
            }
        }
        Ok(Ignore { patterns })
    }

    /// Whether `path` (relative to the tree's folder, `/`-separated) is left alone.
    pub fn skips(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.patterns.iter().any(|p| {
            if p.contains('/') {
                matches(p.as_bytes(), path.as_bytes())
            } else {
                matches(p.as_bytes(), name.as_bytes())
            }
        })
    }
}

/// Glob matching: `**` matches anything, `*` anything but `/`, `?` one character but `/`.
fn matches(p: &[u8], s: &[u8]) -> bool {
    match p {
        [] => s.is_empty(),
        // `**/` is any number of folders, none included.
        [b'*', b'*', b'/', rest @ ..] => {
            matches(rest, s)
                || s.iter()
                    .enumerate()
                    .any(|(i, c)| *c == b'/' && matches(rest, &s[i + 1..]))
        }
        [b'*', b'*', rest @ ..] => (0..=s.len()).any(|i| matches(rest, &s[i..])),
        [b'*', rest @ ..] => {
            for i in 0..=s.len() {
                if matches(rest, &s[i..]) {
                    return true;
                }
                if s.get(i) == Some(&b'/') {
                    break;
                }
            }
            false
        }
        [b'?', rest @ ..] => matches!(s.first(), Some(c) if *c != b'/') && matches(rest, &s[1..]),
        [c, rest @ ..] => s.first() == Some(c) && matches(rest, &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore(patterns: &[&str]) -> Ignore {
        Ignore::new(patterns.iter().map(|p| p.to_string()).collect()).unwrap()
    }

    #[test]
    fn a_name_pattern_matches_at_any_depth() {
        let i = ignore(&["*.md"]);
        assert!(i.skips("README.md"));
        assert!(i.skips("Util/Signal/README.md"));
        assert!(!i.skips("Util/README.md.luau"));
        assert!(!i.skips("md"));
    }

    #[test]
    fn a_path_pattern_matches_from_the_folder() {
        let i = ignore(&["docs/**", "notes/?.txt"]);
        assert!(i.skips("docs/a.png"));
        assert!(i.skips("docs/deep/b.png"));
        assert!(!i.skips("src/docs/a.png"));
        assert!(i.skips("notes/a.txt"));
        assert!(!i.skips("notes/ab.txt"));
        let any = ignore(&["**/LICENSE"]);
        assert!(any.skips("LICENSE"));
        assert!(any.skips("a/b/LICENSE"));
    }

    #[test]
    fn a_star_stays_within_a_name() {
        let i = ignore(&["a/*.md"]);
        assert!(i.skips("a/x.md"));
        assert!(!i.skips("a/b/x.md"));
    }

    #[test]
    fn patterns_that_hide_forges_files_or_leave_the_folder_are_refused() {
        for p in ["*.luau", "*", "_.*", "**", "*.rbxm", "../x", "/abs", ""] {
            assert!(Ignore::new(vec![p.to_string()]).is_err(), "{p}");
        }
    }
}
