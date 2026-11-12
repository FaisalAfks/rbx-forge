//! A push's plan: the operations that make Studio's copy of a tree match the files (decision 0015). forge works the
//! plan out; the plugin shows it and carries it out only after Apply is clicked.
//!
//! Instances are matched by disk path, as in `diff`. Studio's side keeps the ids the plugin gave its instances when it
//! read them, so each operation names an instance the plugin still holds. An instance the files add gets the id it has
//! in the disk tree, marked as new, so references to it can be set once everything exists.
//!
//! Operations (JSON, `op` tag):
//! - `add` a subtree under `parent`, built from the files (MeshParts start from their model file, decision 0011);
//! - `remove` an instance; `move` one to another `parent`, with its `name`;
//! - `set` a property, `reset` one to its default, `attribute` (null removes it), `tags`, `source`, `model`.
//!
//! A reference is `{ "id": n }` for an instance Studio has, `{ "new": n }` for one this push adds, or a list of names
//! from `game` for one outside the tree.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, anyhow};
use base64::Engine;
use serde_json::{Value as Json, json};

use crate::diff::{Index, roots, sorted};
use crate::tree::Node;
use crate::value::{RefTarget, Value};

/// What a push does to one tree.
#[derive(Debug, Default)]
pub struct Plan {
    pub ops: Vec<Json>,
    /// Instances whose values change.
    pub changed: usize,
    /// Instances added, counting everything inside them.
    pub added: usize,
    /// Instances removed, counting everything inside them.
    pub removed: usize,
    pub moved: usize,
    /// Scripts whose text the push replaces, by disk path.
    pub scripts: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// The plan that takes `studio` (with the plugin's ids, and `studio_paths` from `layout::disk_paths`) to `disk` (read
/// from the files).
pub fn plan(studio: &Node, studio_paths: HashMap<u32, String>, disk: &Node) -> Result<Plan> {
    let s = Index::with_paths(studio, studio_paths);
    let d = Index::new(disk);

    // A path whose class changes is replaced whole: everything at and below it goes, and comes back from the files.
    let replaced: BTreeSet<&String> = d
        .nodes
        .iter()
        .filter(|(p, n)| s.nodes.get(*p).is_some_and(|m| m.class != n.class))
        .map(|(p, _)| p)
        .collect();
    let under = |p: &String| {
        replaced
            .iter()
            .any(|r| r.is_empty() || p == *r || p.starts_with(&format!("{r}/")))
    };
    let gone: BTreeSet<&String> = s
        .nodes
        .keys()
        .filter(|p| !d.nodes.contains_key(*p) || under(p))
        .collect();
    let new: BTreeSet<&String> = d
        .nodes
        .keys()
        .filter(|p| !s.nodes.contains_key(*p) || under(p))
        .collect();
    if gone.contains(&String::new()) {
        return Err(anyhow!(
            "the tree's top is a {} in Studio and a {} on disk: push can't replace it",
            studio.class,
            disk.class
        ));
    }

    // Moves: a gone subtree that's on disk whole under another path.
    let mut by_print: HashMap<String, Vec<&String>> = HashMap::new();
    for p in roots(&gone) {
        by_print.entry(s.print(p)?).or_default().push(p);
    }
    let mut moves: Vec<(&String, &String)> = Vec::new();
    for p in roots(&new) {
        if let Some(from) = by_print.get_mut(&d.print(p)?).and_then(|v| v.pop()) {
            moves.push((from, p));
        }
    }
    let moved_from: BTreeSet<&String> = moves.iter().map(|(f, _)| *f).collect();
    let moved_to: BTreeSet<&String> = moves.iter().map(|(_, t)| *t).collect();

    let targets = Targets {
        s: &s,
        d: &d,
        gone: &gone,
        moves: &moves,
    };
    let mut plan = Plan::default();

    for p in roots(&new).into_iter().filter(|p| !moved_to.contains(p)) {
        let node = d.nodes[p];
        plan.ops.push(
            json!({ "op": "add", "parent": targets.parent_of(p)?, "tree": targets.tree(node)? }),
        );
        plan.added += node.walk().len();
        for n in node.walk().into_iter().filter(|n| n.source.is_some()) {
            plan.scripts.push(d.paths[&n.id].clone());
        }
    }
    for (from, to) in &moves {
        let name = &d.nodes[*to].name;
        plan.ops.push(json!({ "op": "move", "id": s.nodes[*from].id, "parent": targets.parent_of(to)?, "name": name }));
        plan.moved += 1;
    }
    for (path, a) in &s.nodes {
        if gone.contains(path) {
            continue;
        }
        let b = d.nodes[path];
        let before = plan.ops.len();
        values(&targets, a, b, &mut plan.ops)?;
        if a.source != b.source {
            plan.ops.push(json!({ "op": "source", "id": a.id, "source": b.source.clone().unwrap_or_default() }));
            plan.scripts.push(path.clone());
        }
        if a.model != b.model
            && let Some(m) = &b.model
        {
            plan.ops.push(json!({ "op": "model", "id": a.id, "model": base64::engine::general_purpose::STANDARD.encode(m) }));
        }
        if plan.ops.len() > before {
            plan.changed += 1;
        }
    }
    for p in roots(&gone).into_iter().filter(|p| !moved_from.contains(p)) {
        plan.ops
            .push(json!({ "op": "remove", "id": s.nodes[p].id }));
        plan.removed += s.nodes[p].walk().len();
    }
    Ok(plan)
}

/// A tree from the files in the form the plugin builds from, with every reference inside it pointing at the copy
/// being built: what the round-trip check sends.
pub fn tree_json(disk: &Node) -> Result<Json> {
    let d = Index::new(disk);
    let empty = Node::default();
    let s = Index::with_paths(&empty, HashMap::new());
    let gone = BTreeSet::new();
    let targets = Targets {
        s: &s,
        d: &d,
        gone: &gone,
        moves: &[],
    };
    targets.tree(disk)
}

/// Properties, attributes and tags that differ, as operations on Studio's instance `a`.
fn values(t: &Targets, a: &Node, b: &Node, ops: &mut Vec<Json>) -> Result<()> {
    for name in a
        .properties
        .keys()
        .chain(b.properties.keys())
        .collect::<BTreeSet<_>>()
    {
        let x = a.properties.get(name).map(|v| t.s.show(v)).transpose()?;
        let y = b.properties.get(name).map(|v| t.d.show(v)).transpose()?;
        if x == y {
            continue;
        }
        match b.properties.get(name) {
            Some(v) => ops.push(json!({ "op": "set", "id": a.id, "name": name, "type": v.ty(), "value": t.value(v)? })),
            None => ops.push(json!({ "op": "reset", "id": a.id, "name": name })),
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
            let value = b.attributes.get(name).map(attribute);
            ops.push(json!({ "op": "attribute", "id": a.id, "name": name, "value": value }));
        }
    }
    if sorted(&a.tags) != sorted(&b.tags) {
        ops.push(json!({ "op": "tags", "id": a.id, "tags": b.tags }));
    }
    Ok(())
}

/// An attribute as the plugin sets it: its type as `typeof` names it, and its value.
fn attribute(v: &Value) -> Json {
    let ty = match v {
        Value::Float(_) | Value::Int(_) => "number".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        other => serde_json::to_value(other.ty())
            .ok()
            .and_then(|j| j.as_str().map(str::to_string))
            .unwrap_or_default(),
    };
    json!({ "type": ty, "value": v.to_json() })
}

/// Where references in the files point, in the terms the plugin knows.
struct Targets<'a> {
    s: &'a Index<'a>,
    d: &'a Index<'a>,
    gone: &'a BTreeSet<&'a String>,
    moves: &'a [(&'a String, &'a String)],
}

impl Targets<'_> {
    /// The instance at disk path `p` after the push: one Studio has (maybe moved there), or one the push adds.
    fn at(&self, p: &str) -> Result<Json> {
        if let Some(n) = self
            .s
            .nodes
            .get(p)
            .filter(|_| !self.gone.contains(&p.to_string()))
        {
            return Ok(json!({ "id": n.id }));
        }
        for (from, to) in self.moves {
            let rest = if p == to.as_str() {
                Some("")
            } else {
                p.strip_prefix(&format!("{to}/"))
            };
            if let Some(rest) = rest {
                let old = if rest.is_empty() {
                    (*from).clone()
                } else {
                    format!("{from}/{rest}")
                };
                if let Some(n) = self.s.nodes.get(&old) {
                    return Ok(json!({ "id": n.id }));
                }
            }
        }
        let n = self
            .d
            .nodes
            .get(p)
            .ok_or_else(|| anyhow!("nothing on disk at {p}"))?;
        Ok(json!({ "new": n.id }))
    }

    fn parent_of(&self, p: &str) -> Result<Json> {
        self.at(p.rsplit_once('/').map_or("", |(parent, _)| parent))
    }

    /// A value in the plugin's JSON form, with references as `at` gives them.
    fn value(&self, v: &Value) -> Result<Json> {
        Ok(match v {
            Value::Ref(RefTarget::Node(id)) => {
                let p = self
                    .d
                    .paths
                    .get(id)
                    .ok_or_else(|| anyhow!("a reference to id {id}, which isn't on disk"))?;
                self.at(p)?
            }
            Value::Ref(RefTarget::Outside(names)) => json!(names),
            other => other.to_json(),
        })
    }

    /// A subtree from the files, in the plugin's JSON form, with each node's disk id as its new id.
    fn tree(&self, n: &Node) -> Result<Json> {
        let mut properties = serde_json::Map::new();
        for (k, v) in &n.properties {
            properties.insert(
                k.clone(),
                json!({ "type": v.ty(), "value": self.value(v)? }),
            );
        }
        let attributes: BTreeMap<&String, Json> = n
            .attributes
            .iter()
            .map(|(k, v)| (k, attribute(v)))
            .collect();
        let mut o = json!({
            "id": n.id,
            "class": n.class,
            "name": n.name,
            "properties": properties,
            "attributes": attributes,
            "tags": n.tags,
            "children": n.children.iter().map(|c| self.tree(c)).collect::<Result<Vec<_>>>()?,
        });
        if let Some(src) = &n.source {
            o["source"] = json!(src);
        }
        if let Some(m) = &n.model {
            o["model"] = json!(base64::engine::general_purpose::STANDARD.encode(m));
        }
        Ok(o)
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

    /// Studio's copy, with the plugin's ids (100 and up), and its disk paths.
    fn studio() -> (Node, HashMap<u32, String>) {
        let mut bar = node(
            102,
            "Frame",
            "Bar",
            vec![node(103, "UICorner", "Corner", vec![])],
        );
        bar.properties.insert("Visible".into(), Value::Bool(false));
        let mut main = node(105, "ModuleScript", "Main", vec![]);
        main.source = Some("return 1\n".into());
        let panel = node(104, "Frame", "Panel", vec![main]);
        let root = node(101, "ScreenGui", "Hud", vec![bar, panel]);
        let paths = [
            (101, ""),
            (102, "Bar"),
            (103, "Bar/Corner"),
            (104, "Panel"),
            (105, "Panel/Main"),
        ]
        .into_iter()
        .map(|(id, p)| (id, p.to_string()))
        .collect();
        (root, paths)
    }

    /// The same tree as read from files (its own ids, 1 and up).
    fn disk() -> Node {
        let mut bar = node(
            2,
            "Frame",
            "Bar",
            vec![node(3, "UICorner", "Corner", vec![])],
        );
        bar.properties.insert("Visible".into(), Value::Bool(false));
        let mut main = node(5, "ModuleScript", "Main", vec![]);
        main.source = Some("return 1\n".into());
        node(
            1,
            "ScreenGui",
            "Hud",
            vec![bar, node(4, "Frame", "Panel", vec![main])],
        )
    }

    fn ops(p: &Plan) -> Vec<String> {
        p.ops.iter().map(|o| o.to_string()).collect()
    }

    #[test]
    fn the_same_tree_needs_nothing() {
        let (s, paths) = studio();
        assert!(plan(&s, paths, &disk()).unwrap().is_empty());
    }

    #[test]
    fn changed_values_become_sets_and_resets() {
        let (s, paths) = studio();
        let mut d = disk();
        d.children[0].properties.remove("Visible");
        d.children[1]
            .properties
            .insert("Size".into(), Value::UDim2(1.0, 0, 0.0, 40));
        d.children[1]
            .attributes
            .insert("Speed".into(), Value::Float(2.0));
        d.children[1].tags = vec!["Hot".into()];
        let p = plan(&s, paths, &d).unwrap();
        assert_eq!(
            ops(&p),
            [
                r#"{"id":102,"name":"Visible","op":"reset"}"#,
                r#"{"id":104,"name":"Size","op":"set","type":"UDim2","value":[1.0,0,0.0,40]}"#,
                r#"{"id":104,"name":"Speed","op":"attribute","value":{"type":"number","value":2.0}}"#,
                r#"{"id":104,"op":"tags","tags":["Hot"]}"#,
            ]
        );
        assert_eq!(p.changed, 2);
    }

    #[test]
    fn script_text_is_its_own_operation() {
        let (s, paths) = studio();
        let mut d = disk();
        d.children[1].children[0].source = Some("return 2\n".into());
        let p = plan(&s, paths, &d).unwrap();
        assert_eq!(
            ops(&p),
            [r#"{"id":105,"op":"source","source":"return 2\n"}"#]
        );
        assert_eq!(p.scripts, ["Panel/Main"]);
    }

    #[test]
    fn added_removed_and_moved_subtrees() {
        let (s, paths) = studio();
        let mut d = disk();
        let bar = d.children.remove(0);
        d.children[0].children.push(bar); // Bar moves under Panel
        d.children.push(node(9, "TextLabel", "Title", vec![])); // Title is new
        d.children[0].children.retain(|c| c.name != "Main"); // Main goes
        let p = plan(&s, paths, &d).unwrap();
        assert_eq!(
            ops(&p),
            [
                r#"{"op":"add","parent":{"id":101},"tree":{"attributes":{},"children":[],"class":"TextLabel","id":9,"name":"Title","properties":{},"tags":[]}}"#,
                r#"{"id":102,"name":"Bar","op":"move","parent":{"id":104}}"#,
                r#"{"id":105,"op":"remove"}"#,
            ]
        );
        assert_eq!((p.added, p.removed, p.moved), (1, 1, 1));
    }

    #[test]
    fn references_point_at_studio_instances_or_new_ones() {
        let (s, paths) = studio();
        let mut d = disk();
        d.children.push(node(9, "ObjectValue", "Link", vec![]));
        d.children[2]
            .properties
            .insert("Value".into(), Value::Ref(RefTarget::Node(4)));
        d.properties
            .insert("PrimaryPart".into(), Value::Ref(RefTarget::Node(9)));
        let p = plan(&s, paths, &d).unwrap();
        let text = ops(&p).join("\n");
        assert!(
            text.contains(r#""Value":{"type":"Ref","value":{"id":104}}"#),
            "{text}"
        );
        assert!(
            text.contains(r#""name":"PrimaryPart","op":"set","type":"Ref","value":{"new":9}"#),
            "{text}"
        );
    }

    #[test]
    fn a_new_class_at_a_path_is_replaced_whole() {
        let (s, paths) = studio();
        let mut d = disk();
        d.children[0].class = "ImageLabel".into();
        let p = plan(&s, paths, &d).unwrap();
        let text = ops(&p).join("\n");
        assert!(text.contains(r#""class":"ImageLabel""#), "{text}");
        assert!(text.contains(r#"{"id":102,"op":"remove"}"#), "{text}");
        assert_eq!((p.added, p.removed), (2, 2));
    }
}
