//! What differs between two copies of a tree, as a list of changes a person can read: the copy on disk and the copy
//! in Studio. Both are compared as layout C has them (each read from files), so an instance is the same instance when
//! it has the same disk path (decision 0007), and values are compared by their written form, which is exact.
//!
//! A subtree that's gone from one path and shows up whole at another is one move or rename, not a removal and an
//! addition. Without watch's live map (decision 0013), that's found by content: the subtree has to match exactly.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;

use crate::tree::Node;
use crate::value::{RefTarget, Value};

/// One difference. Paths are disk paths from the tree's top, `/`-separated (`""` is the top itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// In Studio only, with how many instances it holds below it.
    Added {
        path: String,
        class: String,
        inside: usize,
    },
    /// On disk only.
    Removed {
        path: String,
        class: String,
        inside: usize,
    },
    /// The same subtree under another path: moved to another parent, or renamed.
    Moved {
        from: String,
        to: String,
        class: String,
    },
    /// A property, attribute or the tags, as written on disk (`None` when unset on that side).
    Value {
        path: String,
        what: String,
        disk: Option<String>,
        studio: Option<String>,
    },
    /// A script's text, with the lines added and removed going from disk to Studio.
    Source {
        path: String,
        added: usize,
        removed: usize,
    },
    /// A MeshPart's model file (its mesh or collision data).
    Model { path: String },
    /// The same path holds another class: shown whole, since nothing else carries over.
    Class {
        path: String,
        disk: String,
        studio: String,
    },
}

/// The changes that take `disk` to `studio`.
pub fn diff(disk: &Node, studio: &Node) -> Result<Vec<Change>> {
    let d = Index::new(disk);
    let s = Index::new(studio);
    let mut changes = Vec::new();

    let gone: BTreeSet<&String> = d
        .nodes
        .keys()
        .filter(|p| !s.nodes.contains_key(*p))
        .collect();
    let new: BTreeSet<&String> = s
        .nodes
        .keys()
        .filter(|p| !d.nodes.contains_key(*p))
        .collect();
    let gone_roots: Vec<&String> = roots(&gone);
    let new_roots: Vec<&String> = roots(&new);

    // Moves: a new subtree identical to a gone one. Each gone subtree pairs once.
    let mut by_print: HashMap<String, Vec<&String>> = HashMap::new();
    for p in &gone_roots {
        by_print.entry(d.print(p)?).or_default().push(p);
    }
    let mut moved_from = BTreeSet::new();
    let mut moved_to = BTreeSet::new();
    for p in &new_roots {
        if let Some(from) = by_print.get_mut(&s.print(p)?).and_then(|v| v.pop()) {
            changes.push(Change::Moved {
                from: from.clone(),
                to: (*p).clone(),
                class: s.nodes[*p].class.clone(),
            });
            moved_from.insert(from);
            moved_to.insert(*p);
        }
    }
    for p in gone_roots.iter().filter(|p| !moved_from.contains(*p)) {
        let n = d.nodes[*p];
        changes.push(Change::Removed {
            path: (*p).clone(),
            class: n.class.clone(),
            inside: n.walk().len() - 1,
        });
    }
    for p in new_roots.iter().filter(|p| !moved_to.contains(*p)) {
        let n = s.nodes[*p];
        changes.push(Change::Added {
            path: (*p).clone(),
            class: n.class.clone(),
            inside: n.walk().len() - 1,
        });
    }

    for (path, a) in &d.nodes {
        let Some(b) = s.nodes.get(path) else { continue };
        if a.class != b.class {
            changes.push(Change::Class {
                path: path.clone(),
                disk: a.class.clone(),
                studio: b.class.clone(),
            });
            continue;
        }
        compare(path, a, b, &d, &s, &mut changes)?;
    }
    changes.sort_by(|x, y| order(x).cmp(&order(y)));
    Ok(changes)
}

/// Sorted by path, so the list reads top to bottom like the tree.
fn order(c: &Change) -> (&str, u8) {
    match c {
        Change::Removed { path, .. } => (path, 0),
        Change::Moved { to, .. } => (to, 1),
        Change::Added { path, .. } => (path, 2),
        Change::Class { path, .. } => (path, 3),
        Change::Value { path, .. } => (path, 4),
        Change::Source { path, .. } => (path, 5),
        Change::Model { path } => (path, 6),
    }
}

fn compare(
    path: &str,
    a: &Node,
    b: &Node,
    d: &Index,
    s: &Index,
    out: &mut Vec<Change>,
) -> Result<()> {
    let value = |what: String, disk: Option<String>, studio: Option<String>| Change::Value {
        path: path.to_string(),
        what,
        disk,
        studio,
    };
    for name in a
        .properties
        .keys()
        .chain(b.properties.keys())
        .collect::<BTreeSet<_>>()
    {
        let x = a.properties.get(name).map(|v| d.show(v)).transpose()?;
        let y = b.properties.get(name).map(|v| s.show(v)).transpose()?;
        if x != y {
            out.push(value(name.clone(), x, y));
        }
    }
    for name in a
        .attributes
        .keys()
        .chain(b.attributes.keys())
        .collect::<BTreeSet<_>>()
    {
        let x = a
            .attributes
            .get(name)
            .map(|v| v.attribute_to_toml())
            .transpose()?;
        let y = b
            .attributes
            .get(name)
            .map(|v| v.attribute_to_toml())
            .transpose()?;
        if x != y {
            out.push(value(format!("attribute {name}"), x, y));
        }
    }
    let tags = |n: &Node| (!n.tags.is_empty()).then(|| format!("{:?}", sorted(&n.tags)));
    if tags(a) != tags(b) {
        out.push(value("tags".into(), tags(a), tags(b)));
    }
    if a.source != b.source {
        let (added, removed) = lines(
            a.source.as_deref().unwrap_or(""),
            b.source.as_deref().unwrap_or(""),
        );
        out.push(Change::Source {
            path: path.to_string(),
            added,
            removed,
        });
    }
    if a.model != b.model {
        out.push(Change::Model {
            path: path.to_string(),
        });
    }
    Ok(())
}

pub(crate) fn sorted(tags: &[String]) -> Vec<&String> {
    let mut t: Vec<&String> = tags.iter().collect();
    t.sort();
    t
}

/// Lines added and removed from `a` to `b`.
fn lines(a: &str, b: &str) -> (usize, usize) {
    let diff = similar::TextDiff::from_lines(a, b);
    let (mut added, mut removed) = (0, 0);
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

/// Paths whose parent isn't in the set: the tops of the subtrees the set covers.
pub(crate) fn roots<'a>(set: &BTreeSet<&'a String>) -> Vec<&'a String> {
    set.iter()
        .filter(|p| match p.rsplit_once('/') {
            Some((parent, _)) => !set.contains(&parent.to_string()),
            None => p.is_empty() || !set.contains(&String::new()),
        })
        .copied()
        .collect()
}

/// One copy of the tree, by disk path.
pub(crate) struct Index<'a> {
    pub(crate) nodes: BTreeMap<String, &'a Node>,
    /// Each node's path by id, for writing references as paths.
    pub(crate) paths: HashMap<u32, String>,
}

impl<'a> Index<'a> {
    /// A tree read from files, whose nodes carry their disk names.
    pub(crate) fn new(root: &'a Node) -> Index<'a> {
        let mut index = Index {
            nodes: BTreeMap::new(),
            paths: HashMap::new(),
        };
        index.add(root, String::new());
        index
    }

    /// A tree with its disk paths given by id (`layout::disk_paths`), keeping its own ids.
    pub(crate) fn with_paths(root: &'a Node, paths: HashMap<u32, String>) -> Index<'a> {
        let mut nodes = BTreeMap::new();
        for n in root.walk() {
            if let Some(p) = paths.get(&n.id) {
                nodes.insert(p.clone(), n);
            }
        }
        Index { nodes, paths }
    }

    fn add(&mut self, node: &'a Node, path: String) {
        for c in &node.children {
            let key = c.key.clone().unwrap_or_else(|| c.name.clone());
            let child = if path.is_empty() {
                key
            } else {
                format!("{path}/{key}")
            };
            self.add(c, child);
        }
        self.paths.insert(node.id, path.clone());
        self.nodes.insert(path, node);
    }

    /// A value as written on disk, with references inside the tree as `@path`.
    pub(crate) fn show(&self, v: &Value) -> Result<String> {
        v.to_toml(&|t| {
            Ok(match t {
                RefTarget::Node(id) => match self.paths.get(id) {
                    Some(p) => format!("@{p}"),
                    None => format!("@?{id}"),
                },
                RefTarget::Outside(names) => format!("@/{}", names.join("/")),
                RefTarget::Path(p) => p.clone(),
            })
        })
    }

    /// A subtree's content, for finding the same subtree under another path. The top's own name is left out, so a
    /// rename matches; references are written relative to the subtree's top, so a moved subtree's inner links match.
    pub(crate) fn print(&self, path: &str) -> Result<String> {
        let mut out = String::new();
        self.print_into(self.nodes[path], path, path, true, &mut out)?;
        Ok(out)
    }

    fn print_into(
        &self,
        n: &Node,
        top: &str,
        at: &str,
        is_top: bool,
        out: &mut String,
    ) -> Result<()> {
        out.push_str(&n.class);
        if !is_top {
            out.push(' ');
            out.push_str(at.strip_prefix(top).unwrap_or(at));
        }
        for (k, v) in &n.properties {
            let shown = self.show(v)?;
            let shown = shown.replace(&format!("\"@{top}"), "\"@~");
            out.push_str(&format!("\n{k}={shown}"));
        }
        for (k, v) in &n.attributes {
            out.push_str(&format!("\n@{k}={}", v.attribute_to_toml()?));
        }
        out.push_str(&format!(
            "\n#{:?}\n{:?}\n{:?}\n",
            sorted(&n.tags),
            n.source,
            n.model.as_ref().map(|m| m.len())
        ));
        let mut kids: Vec<(String, &Node)> = n
            .children
            .iter()
            .map(|c| {
                let key = c.key.clone().unwrap_or_else(|| c.name.clone());
                (
                    if at.is_empty() {
                        key
                    } else {
                        format!("{at}/{key}")
                    },
                    c,
                )
            })
            .collect();
        kids.sort_by(|x, y| x.0.cmp(&y.0));
        for (p, c) in kids {
            out.push('{');
            self.print_into(c, top, &p, false, out)?;
            out.push('}');
        }
        Ok(())
    }
}

impl Change {
    /// The change as a push describes it, for changes worked out from Studio's copy to the files: what the push does
    /// to Studio, old value first.
    pub fn push_line(&self) -> String {
        let at = |p: &str| {
            if p.is_empty() {
                "(top)".to_string()
            } else {
                p.to_string()
            }
        };
        let inside = |n: usize| {
            if n == 0 {
                String::new()
            } else {
                format!(", {n} inside")
            }
        };
        match self {
            Change::Added {
                path,
                class,
                inside: n,
            } => format!("+ {} ({class}{}): added", at(path), inside(*n)),
            Change::Removed {
                path,
                class,
                inside: n,
            } => format!("- {} ({class}{}): removed", at(path), inside(*n)),
            Change::Moved { from, to, class } => {
                format!("> {} -> {} ({class}): moved or renamed", at(from), at(to))
            }
            Change::Value {
                path,
                what,
                disk,
                studio,
            } => format!(
                "~ {} {what}: {} -> {}",
                at(path),
                disk.as_deref().unwrap_or("(unset)"),
                studio.as_deref().unwrap_or("(unset)")
            ),
            Change::Source {
                path,
                added,
                removed,
            } => format!("~ {} script: +{added} -{removed} lines", at(path)),
            Change::Model { path } => format!("~ {} model file: replaced", at(path)),
            Change::Class { path, disk, studio } => {
                format!("! {}: the {disk} is replaced by a {studio}", at(path))
            }
        }
    }
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let at = |p: &str| {
            if p.is_empty() {
                "(top)".to_string()
            } else {
                p.to_string()
            }
        };
        let inside = |n: usize| {
            if n == 0 {
                String::new()
            } else {
                format!(", {n} inside")
            }
        };
        match self {
            Change::Added {
                path,
                class,
                inside: n,
            } => write!(f, "+ {} ({class}{}): in Studio only", at(path), inside(*n)),
            Change::Removed {
                path,
                class,
                inside: n,
            } => write!(f, "- {} ({class}{}): on disk only", at(path), inside(*n)),
            Change::Moved { from, to, class } => write!(
                f,
                "> {} -> {} ({class}): moved or renamed",
                at(from),
                at(to)
            ),
            Change::Value {
                path,
                what,
                disk,
                studio,
            } => write!(
                f,
                "~ {} {what}: {} -> {}",
                at(path),
                disk.as_deref().unwrap_or("(unset)"),
                studio.as_deref().unwrap_or("(unset)")
            ),
            Change::Source {
                path,
                added,
                removed,
            } => {
                write!(f, "~ {} script: +{added} -{removed} lines", at(path))
            }
            Change::Model { path } => write!(
                f,
                "~ {} model file: its mesh or collision data changed",
                at(path)
            ),
            Change::Class { path, disk, studio } => write!(
                f,
                "! {}: class {disk} on disk, {studio} in Studio",
                at(path)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, class: &str, name: &str, children: Vec<Node>) -> Node {
        Node {
            id,
            class: class.into(),
            name: name.into(),
            key: Some(name.into()),
            children,
            ..Node::default()
        }
    }

    fn hud() -> Node {
        let mut bar = node(
            2,
            "Frame",
            "Bar",
            vec![node(3, "UICorner", "Corner", vec![])],
        );
        bar.properties
            .insert("BackgroundColor3".into(), Value::Color3([0.0, 0.0, 0.0]));
        let mut main = node(5, "ModuleScript", "Main", vec![]);
        main.source = Some("local a = 1\nlocal b = 2\nreturn a + b\n".into());
        let panel = node(4, "Frame", "Panel", vec![main]);
        node(1, "ScreenGui", "Hud", vec![bar, panel])
    }

    fn lines_of(changes: &[Change]) -> Vec<String> {
        changes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn the_same_tree_has_no_changes() {
        assert!(diff(&hud(), &hud()).unwrap().is_empty());
    }

    #[test]
    fn a_colour_change_is_one_line() {
        let mut studio = hud();
        studio.children[0]
            .properties
            .insert("BackgroundColor3".into(), Value::Color3([1.0, 0.0, 0.0]));
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            ["~ Bar BackgroundColor3: \"#000000\" -> \"#ff0000\""]
        );
    }

    #[test]
    fn a_moved_frame_is_one_line() {
        let mut studio = hud();
        let bar = studio.children.remove(0);
        studio.children[0].children.push(bar);
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            ["> Bar -> Panel/Bar (Frame): moved or renamed"]
        );
    }

    #[test]
    fn a_rename_is_one_line() {
        let mut studio = hud();
        studio.children[0].name = "Meter".into();
        studio.children[0].key = Some("Meter".into());
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            ["> Bar -> Meter (Frame): moved or renamed"]
        );
    }

    #[test]
    fn an_edited_script_is_one_line_with_its_line_counts() {
        let mut studio = hud();
        studio.children[1].children[0].source =
            Some("local a = 1\nlocal b = 3\nreturn a + b\n".into());
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            ["~ Panel/Main script: +1 -1 lines"]
        );
    }

    #[test]
    fn added_and_removed_subtrees_are_one_line_each() {
        let mut studio = hud();
        studio.children.remove(0);
        studio.children.push(node(
            9,
            "TextLabel",
            "Title",
            vec![node(10, "UIStroke", "Stroke", vec![])],
        ));
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            [
                "- Bar (Frame, 1 inside): on disk only",
                "+ Title (TextLabel, 1 inside): in Studio only"
            ]
        );
    }

    #[test]
    fn attributes_tags_and_models() {
        let mut studio = hud();
        studio.children[0]
            .attributes
            .insert("Speed".into(), Value::Float(2.0));
        studio.children[0].tags = vec!["Hot".into()];
        studio.children[1].model = Some(vec![1, 2, 3]);
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            [
                "~ Bar attribute Speed: (unset) -> 2",
                "~ Bar tags: (unset) -> [\"Hot\"]",
                "~ Panel model file: its mesh or collision data changed"
            ]
        );
    }

    #[test]
    fn references_are_shown_as_paths() {
        let mut disk = hud();
        disk.properties
            .insert("PrimaryPart".into(), Value::Ref(RefTarget::Node(2)));
        let mut studio = hud();
        studio
            .properties
            .insert("PrimaryPart".into(), Value::Ref(RefTarget::Node(4)));
        assert_eq!(
            lines_of(&diff(&disk, &studio).unwrap()),
            ["~ (top) PrimaryPart: \"@Bar\" -> \"@Panel\""]
        );
    }

    #[test]
    fn a_new_class_at_the_same_path_is_flagged() {
        let mut studio = hud();
        studio.children[0].class = "ImageLabel".into();
        assert_eq!(
            lines_of(&diff(&hud(), &studio).unwrap()),
            ["! Bar: class Frame on disk, ImageLabel in Studio"]
        );
    }
}
