//! Watch's three-way merge (decision 0013): Studio's copy of a tree and the files are each compared with the last
//! state both sides agreed on, value by value. A value only one side changed goes to the other side; a value both
//! sides changed the same way is simply agreed; a value they changed differently is a clash, and neither is applied:
//! each side keeps its own until a side is picked.
//!
//! Trees are compared flat, by disk path, as diff does. A value is compared in its written form, so a change that puts
//! a value back where it was, or forge's own write coming back as an event, merges to nothing: that's how echoes are
//! dropped, by comparing values, not with flags.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;

use crate::diff::{Index, sorted};
use crate::tree::Node;
use crate::value::{RefTarget, Value};

/// One instance, by what can change on it. References inside the tree point by disk path, so they survive the trip.
/// Two entries are equal when their written forms are: values are compared as written, which is exact.
#[derive(Debug, Clone)]
pub struct Entry {
    pub class: String,
    pub name: String,
    /// Each value with its written form, which is what's compared.
    pub fields: BTreeMap<String, (String, Field)>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Entry) -> bool {
        self.class == other.class
            && self.name == other.name
            && self.fields.len() == other.fields.len()
            && self
                .fields
                .iter()
                .zip(&other.fields)
                .all(|((k, (a, _)), (l, (b, _)))| k == l && a == b)
    }
}

#[derive(Debug, Clone)]
pub enum Field {
    Property(Value),
    Attribute(Value),
    Tags(Vec<String>),
    Source(String),
    Model(Vec<u8>),
}

/// A tree flattened by disk path (`""` is its top).
pub type Flat = BTreeMap<String, Entry>;

/// A value both sides changed differently since they last agreed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Clash {
    pub path: String,
    /// The property (`Size`), `attribute Name`, `tags`, `source`, `model`, or `instance` for the instance as a whole.
    pub what: String,
    pub studio: Option<String>,
    pub disk: Option<String>,
}

/// What a merge gives each side, and the new agreed state.
#[derive(Debug, Default)]
pub struct Merged {
    /// What the files should hold: everything merged, and the disk's own value where there's a clash.
    pub to_disk: Flat,
    /// What Studio should hold: everything merged, and Studio's own value where there's a clash.
    pub to_studio: Flat,
    /// The new agreed state: everything merged, and the old agreed value where there's a clash.
    pub base: Flat,
    pub clashes: Vec<Clash>,
}

/// A tree (as read from files, with disk names) flattened by disk path.
pub fn flatten(tree: &Node) -> Result<Flat> {
    let index = Index::new(tree);
    let mut flat = Flat::new();
    for (path, node) in &index.nodes {
        let mut fields = BTreeMap::new();
        for (k, v) in &node.properties {
            fields.insert(
                k.clone(),
                (index.show(v)?, Field::Property(by_path(v, &index))),
            );
        }
        for (k, v) in &node.attributes {
            fields.insert(
                format!("attribute {k}"),
                (v.attribute_to_toml()?, Field::Attribute(v.clone())),
            );
        }
        if !node.tags.is_empty() {
            let tags: Vec<String> = sorted(&node.tags).into_iter().cloned().collect();
            fields.insert("tags".into(), (format!("{tags:?}"), Field::Tags(tags)));
        }
        if let Some(s) = &node.source {
            fields.insert("source".into(), (s.clone(), Field::Source(s.clone())));
        }
        if let Some(m) = &node.model {
            fields.insert(
                "model".into(),
                (
                    format!("{:x}", crate::names::fnv(m)),
                    Field::Model(m.clone()),
                ),
            );
        }
        flat.insert(
            path.clone(),
            Entry {
                class: node.class.clone(),
                name: node.name.clone(),
                fields,
            },
        );
    }
    Ok(flat)
}

/// A reference to an instance in the tree, by its disk path instead of an id.
fn by_path(v: &Value, index: &Index) -> Value {
    match v {
        Value::Ref(RefTarget::Node(id)) => match index.paths.get(id) {
            Some(p) => Value::Ref(RefTarget::Path(p.clone())),
            None => v.clone(),
        },
        other => other.clone(),
    }
}

/// A flat tree back as a tree, with disk names kept as each node's key so the files keep their names.
pub fn unflatten(flat: &Flat) -> Result<Node> {
    let ids: HashMap<&String, u32> = flat.keys().zip(1..).collect();
    let mut nodes: BTreeMap<&String, Node> = BTreeMap::new();
    for (path, e) in flat {
        let mut node = Node {
            id: ids[path],
            class: e.class.clone(),
            name: e.name.clone(),
            key: path
                .rsplit('/')
                .next()
                .filter(|k| !path.is_empty() && !k.is_empty())
                .map(str::to_string),
            ..Node::default()
        };
        for (k, (_, f)) in &e.fields {
            match f {
                Field::Property(Value::Ref(RefTarget::Path(p))) => {
                    // A reference to an instance that's gone is dropped, as Studio does when its target is removed.
                    if let Some(id) = ids.get(p) {
                        node.properties
                            .insert(k.clone(), Value::Ref(RefTarget::Node(*id)));
                    }
                }
                Field::Property(v) => {
                    node.properties.insert(k.clone(), v.clone());
                }
                Field::Attribute(v) => {
                    node.attributes
                        .insert(k.trim_start_matches("attribute ").to_string(), v.clone());
                }
                Field::Tags(t) => node.tags = t.clone(),
                Field::Source(s) => node.source = Some(s.clone()),
                Field::Model(m) => node.model = Some(m.clone()),
            }
        }
        nodes.insert(path, node);
    }
    // Children into parents, deepest first, so each node is whole when it's moved under its parent.
    let mut paths: Vec<&String> = nodes.keys().copied().collect();
    paths.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count() + usize::from(!p.is_empty())));
    for p in paths {
        if p.is_empty() {
            continue;
        }
        let parent = p
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_string();
        let child = nodes.remove(p).expect("each path once");
        match nodes.get_mut(&parent) {
            Some(n) => n.children.push(child),
            None => anyhow::bail!("{p} has no parent {parent:?} in the merged tree"),
        }
    }
    for n in nodes.values_mut() {
        sort_children(n);
    }
    nodes
        .remove(&String::new())
        .ok_or_else(|| anyhow::anyhow!("the merged tree has no top"))
}

fn sort_children(n: &mut Node) {
    n.children.sort_by(|a, b| a.key.cmp(&b.key));
    for c in &mut n.children {
        sort_children(c);
    }
}

/// Merges Studio's copy and the files against the state they last agreed on.
pub fn merge(base: &Flat, studio: &Flat, disk: &Flat) -> Merged {
    let mut m = Merged::default();
    let paths: BTreeSet<&String> = base
        .keys()
        .chain(studio.keys())
        .chain(disk.keys())
        .collect();
    for p in paths {
        let (b, s, d) = (base.get(p), studio.get(p), disk.get(p));
        match (s, d) {
            (Some(s), Some(d)) if s.class == d.class => {
                let merged = fields(p, b.filter(|b| b.class == s.class), s, d, &mut m.clashes);
                m.to_disk.insert(p.clone(), merged.0);
                m.to_studio.insert(p.clone(), merged.1);
                if let Some(base) = merged.2 {
                    m.base.insert(p.clone(), base);
                }
            }
            _ => whole(p, b, s, d, &mut m),
        }
    }
    m
}

/// An instance that one side added, removed, or gave another class: the whole instance is taken from the side that
/// changed it, or held as a clash when both changed it.
fn whole(p: &str, b: Option<&Entry>, s: Option<&Entry>, d: Option<&Entry>, m: &mut Merged) {
    let take = |m: &mut Merged, e: Option<&Entry>| {
        if let Some(e) = e {
            m.to_disk.insert(p.to_string(), e.clone());
            m.to_studio.insert(p.to_string(), e.clone());
            m.base.insert(p.to_string(), e.clone());
        }
    };
    if s == d {
        take(m, s);
    } else if s == b {
        take(m, d);
    } else if d == b {
        take(m, s);
    } else {
        let show = |e: Option<&Entry>| e.map(|e| format!("a {}", e.class));
        m.clashes.push(Clash {
            path: p.to_string(),
            what: "instance".into(),
            studio: show(s),
            disk: show(d),
        });
        if let Some(d) = d {
            m.to_disk.insert(p.to_string(), d.clone());
        }
        if let Some(s) = s {
            m.to_studio.insert(p.to_string(), s.clone());
        }
        if let Some(b) = b {
            m.base.insert(p.to_string(), b.clone());
        }
    }
}

/// One instance present on both sides with the same class: each value merged three ways.
fn fields(
    p: &str,
    b: Option<&Entry>,
    s: &Entry,
    d: &Entry,
    clashes: &mut Vec<Clash>,
) -> (Entry, Entry, Option<Entry>) {
    let mut to_disk = Entry {
        class: s.class.clone(),
        name: d.name.clone(),
        fields: BTreeMap::new(),
    };
    let mut to_studio = to_disk.clone();
    let mut base = b.cloned().map(|mut e| {
        e.fields.clear();
        e
    });
    // The name is a value like any other: a rename on one side goes to the other.
    let name = pick(b.map(|b| &b.name), Some(&s.name), Some(&d.name));
    match name {
        Pick::Agreed(n) => {
            let n = n.cloned().unwrap_or_default();
            to_disk.name = n.clone();
            to_studio.name = n.clone();
            if let Some(base) = &mut base {
                base.name = n;
            }
        }
        Pick::Clash => {
            clashes.push(Clash {
                path: p.into(),
                what: "name".into(),
                studio: Some(s.name.clone()),
                disk: Some(d.name.clone()),
            });
            to_disk.name = d.name.clone();
            to_studio.name = s.name.clone();
        }
    }
    let keys: BTreeSet<&String> = s
        .fields
        .keys()
        .chain(d.fields.keys())
        .chain(b.into_iter().flat_map(|b| b.fields.keys()))
        .collect();
    for k in keys {
        let (bf, sf, df) = (
            b.and_then(|b| b.fields.get(k)),
            s.fields.get(k),
            d.fields.get(k),
        );
        match pick(shown(bf), shown(sf), shown(df)) {
            Pick::Agreed(v) => {
                let chosen = if v == shown(sf) { sf } else { df };
                if let Some(f) = chosen {
                    to_disk.fields.insert(k.clone(), f.clone());
                    to_studio.fields.insert(k.clone(), f.clone());
                    if let Some(base) = &mut base {
                        base.fields.insert(k.clone(), f.clone());
                    }
                }
            }
            Pick::Clash => {
                clashes.push(Clash {
                    path: p.into(),
                    what: k.clone(),
                    studio: sf.map(|f| f.0.clone()),
                    disk: df.map(|f| f.0.clone()),
                });
                if let Some(f) = df {
                    to_disk.fields.insert(k.clone(), f.clone());
                }
                if let Some(f) = sf {
                    to_studio.fields.insert(k.clone(), f.clone());
                }
                if let (Some(base), Some(f)) = (&mut base, bf) {
                    base.fields.insert(k.clone(), f.clone());
                }
            }
        }
    }
    // An instance both sides added is agreed from here on, as far as they agree.
    let base = base.or_else(|| Some(to_disk.clone()));
    (to_disk, to_studio, base)
}

fn shown(f: Option<&(String, Field)>) -> Option<&String> {
    f.map(|f| &f.0)
}

enum Pick<'a, T> {
    Agreed(Option<&'a T>),
    Clash,
}

/// Three-way choice of one value: whichever side changed it, or a clash when both changed it differently.
fn pick<'a, T: PartialEq>(b: Option<&'a T>, s: Option<&'a T>, d: Option<&'a T>) -> Pick<'a, T> {
    if s == d {
        Pick::Agreed(s)
    } else if s == b {
        Pick::Agreed(d)
    } else if d == b {
        Pick::Agreed(s)
    } else {
        Pick::Clash
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
        let mut bar = node(2, "Frame", "Bar", vec![]);
        bar.properties.insert("Visible".into(), Value::Bool(true));
        bar.properties.insert("ZIndex".into(), Value::Int(1));
        let mut main = node(3, "ModuleScript", "Main", vec![]);
        main.source = Some("return 1\n".into());
        let mut root = node(1, "ScreenGui", "Hud", vec![bar, main]);
        root.key = None;
        root
    }

    fn flat(n: &Node) -> Flat {
        flatten(n).unwrap()
    }

    #[test]
    fn nothing_changed_merges_to_the_same() {
        let b = flat(&hud());
        let m = merge(&b, &b, &b);
        assert!(m.clashes.is_empty());
        assert_eq!(m.to_disk, b);
        assert_eq!(m.to_studio, b);
        assert_eq!(m.base, b);
    }

    #[test]
    fn a_change_on_one_side_goes_to_the_other() {
        let b = flat(&hud());
        let mut s = hud();
        s.children[0]
            .properties
            .insert("Visible".into(), Value::Bool(false));
        let mut d = hud();
        d.children[1].source = Some("return 2\n".into());
        let m = merge(&b, &flat(&s), &flat(&d));
        assert!(m.clashes.is_empty());
        assert_eq!(m.to_disk, m.to_studio);
        assert_eq!(m.to_disk["Bar"].fields["Visible"].0, "false");
        assert_eq!(m.to_disk["Main"].fields["source"].0, "return 2\n");
    }

    #[test]
    fn different_properties_of_one_instance_both_go_through() {
        let b = flat(&hud());
        let mut s = hud();
        s.children[0]
            .properties
            .insert("Visible".into(), Value::Bool(false));
        let mut d = hud();
        d.children[0]
            .properties
            .insert("ZIndex".into(), Value::Int(5));
        let m = merge(&b, &flat(&s), &flat(&d));
        assert!(m.clashes.is_empty());
        assert_eq!(m.to_disk["Bar"].fields["Visible"].0, "false");
        assert_eq!(m.to_disk["Bar"].fields["ZIndex"].0, "5");
    }

    #[test]
    fn the_same_property_changed_differently_is_held() {
        let b = flat(&hud());
        let mut s = hud();
        s.children[0]
            .properties
            .insert("ZIndex".into(), Value::Int(2));
        let mut d = hud();
        d.children[0]
            .properties
            .insert("ZIndex".into(), Value::Int(3));
        let m = merge(&b, &flat(&s), &flat(&d));
        assert_eq!(
            m.clashes,
            [Clash {
                path: "Bar".into(),
                what: "ZIndex".into(),
                studio: Some("2".into()),
                disk: Some("3".into())
            }]
        );
        // Each side keeps its own; the agreed state keeps the old value.
        assert_eq!(m.to_studio["Bar"].fields["ZIndex"].0, "2");
        assert_eq!(m.to_disk["Bar"].fields["ZIndex"].0, "3");
        assert_eq!(m.base["Bar"].fields["ZIndex"].0, "1");
    }

    #[test]
    fn an_echo_merges_to_nothing() {
        // Studio sends back the value the files already gave it: both sides agree, nothing moves.
        let b = flat(&hud());
        let mut both = hud();
        both.children[0]
            .properties
            .insert("ZIndex".into(), Value::Int(9));
        let m = merge(&b, &flat(&both), &flat(&both));
        assert!(m.clashes.is_empty());
        assert_eq!(m.to_disk, flat(&both));
        assert_eq!(m.base, flat(&both));
    }

    #[test]
    fn added_and_removed_instances_follow_the_side_that_changed() {
        let b = flat(&hud());
        let mut s = hud();
        s.children.push(node(9, "TextLabel", "Title", vec![]));
        let mut d = hud();
        d.children.remove(1);
        let m = merge(&b, &flat(&s), &flat(&d));
        assert!(m.clashes.is_empty());
        assert!(m.to_disk.contains_key("Title"));
        assert!(!m.to_studio.contains_key("Main"));
    }

    #[test]
    fn removed_on_one_side_and_changed_on_the_other_is_held() {
        let b = flat(&hud());
        let mut s = hud();
        s.children.remove(1);
        let mut d = hud();
        d.children[1].source = Some("return 3\n".into());
        let m = merge(&b, &flat(&s), &flat(&d));
        assert_eq!(m.clashes.len(), 1);
        assert_eq!(m.clashes[0].what, "instance");
        assert!(!m.to_studio.contains_key("Main"));
        assert!(m.to_disk.contains_key("Main"));
    }

    #[test]
    fn a_flat_tree_comes_back_with_its_references() {
        let mut tree = hud();
        tree.properties
            .insert("PrimaryPart".into(), Value::Ref(RefTarget::Node(2)));
        let back = unflatten(&flat(&tree)).unwrap();
        assert_eq!(flat(&back), flat(&tree));
        assert_eq!(back.children.len(), 2);
    }
}
