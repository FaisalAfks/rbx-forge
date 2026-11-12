//! Layout C (decision 0008): a tree as files and back. An instance with children is a folder with a `_.toml`
//! (its own class, properties, attributes and tags, plus one section per child without children). A script's text
//! is `<name>.luau` and a MeshPart's model is `<name>.rbxm`, both in the parent's folder (the root's are `_.luau` and
//! `_.rbxm` in its own folder). Every file is written whole, in one fixed form (decision 0015), so the same tree is
//! always the same bytes. `docs/sync/format.md` is the reference.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::names;
use crate::schema::Schema;
use crate::text;
use crate::tree::Node;
use crate::value::{RefTarget, Value};

/// Files of one synced tree: paths relative to its folder, with `/`, and their bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

const PROPS_FILE: &str = "_.toml";

// ---------------------------------------------------------------------------------------------------------------------
// Writing

/// The files for `tree`. Keys read from disk earlier (`Node::key`) are kept where they still fit.
pub fn write(tree: &Node, schema: &Schema) -> Result<Files> {
    tree.check_ids()?;
    let mut keys = KeyMap::default();
    assign_keys(tree, schema, &mut Vec::new(), &mut keys);
    let mut files = Files::new();
    write_folder(tree, "", None, &keys, &mut files)?;
    Ok(files)
}

/// Each node's disk path from the root, `/`-separated (the root's is empty), by id: the paths `write` gives them.
pub fn disk_paths(tree: &Node, schema: &Schema) -> HashMap<u32, String> {
    let mut keys = KeyMap::default();
    assign_keys(tree, schema, &mut Vec::new(), &mut keys);
    keys.paths
        .into_iter()
        .map(|(id, p)| (id, p.join("/")))
        .collect()
}

/// Each node's path of disk names from the root (the root's is empty).
#[derive(Default)]
struct KeyMap {
    paths: HashMap<u32, Vec<String>>,
}

impl KeyMap {
    fn key(&self, id: u32) -> &str {
        self.paths[&id].last().map(String::as_str).unwrap_or("")
    }

    fn reference(&self, target: &RefTarget) -> Result<String> {
        match target {
            RefTarget::Node(id) => {
                let path = self.paths.get(id).ok_or_else(|| {
                    anyhow!("a reference points at id {id}, which isn't in the tree")
                })?;
                Ok(format!("@{}", path.join("/")))
            }
            RefTarget::Outside(names) => Ok(format!(
                "@/{}",
                names
                    .iter()
                    .map(|n| names::outside(n))
                    .collect::<Vec<_>>()
                    .join("/")
            )),
            RefTarget::Path(p) => bail!("unresolved reference {p}"),
        }
    }
}

fn assign_keys(node: &Node, schema: &Schema, path: &mut Vec<String>, keys: &mut KeyMap) {
    keys.paths.insert(node.id, path.clone());
    let clashes =
        |k: &str| schema.property(&node.class, k).is_some() || node.properties.contains_key(k);
    let bases: Vec<String> = node
        .children
        .iter()
        .map(|c| names::base(&c.name, &clashes))
        .collect();
    let pins: Vec<Option<String>> = node.children.iter().map(|c| c.key.clone()).collect();
    let prints: Vec<u64> = node.children.iter().map(fingerprint).collect();
    let mut order: Vec<usize> = (0..node.children.len()).collect();
    order.sort_by_key(|&i| (&node.children[i].class, prints[i]));
    let child_keys = names::assign(&bases, &pins, &order);
    for (child, key) in node.children.iter().zip(child_keys) {
        path.push(key);
        assign_keys(child, schema, path, keys);
        path.pop();
    }
}

/// A hash of a node's content (not its id or disk name), so repeats are numbered the same way every time.
fn fingerprint(node: &Node) -> u64 {
    // Tags are a set; Studio doesn't promise their order.
    let mut tags = node.tags.clone();
    tags.sort();
    let mut s = format!("{}\n{}\n{tags:?}\n", node.class, node.name);
    let r: crate::value::RefWriter = &|_| Ok("ref".into());
    for (k, v) in node.properties.iter().chain(&node.attributes) {
        s.push_str(&format!("{k}={}\n", v.to_toml(r).unwrap_or_default()));
    }
    s.push_str(node.source.as_deref().unwrap_or(""));
    s.push_str(&format!("{:?}", node.model.as_ref().map(|m| names::fnv(m))));
    let mut kids: Vec<u64> = node.children.iter().map(fingerprint).collect();
    kids.sort_unstable();
    s.push_str(&format!("{kids:?}"));
    names::fnv(s.as_bytes())
}

fn join(dir: &str, file: &str) -> String {
    if dir.is_empty() {
        file.to_string()
    } else {
        format!("{dir}/{file}")
    }
}

/// Writes a node that becomes a folder: its `_.toml` (with its leaf children) and everything under it.
fn write_folder(
    node: &Node,
    dir: &str,
    key: Option<&str>,
    keys: &KeyMap,
    files: &mut Files,
) -> Result<()> {
    if key.is_none() {
        // The root's own text and model live in its own folder; it has no parent folder in the tree.
        add_extras(node, dir, "_", files);
    }
    let mut toml = body(node, key, keys)?;
    if !node.attributes.is_empty() {
        toml.push_str("\n[attributes]\n");
        toml.push_str(&attributes(node)?);
    }
    let mut leaves: Vec<&Node> = node
        .children
        .iter()
        .filter(|c| c.children.is_empty())
        .collect();
    leaves.sort_by(|a, b| keys.key(a.id).cmp(keys.key(b.id)));
    for leaf in leaves {
        let k = keys.key(leaf.id);
        toml.push_str(&format!("\n[{}]\n", text::key(k)));
        toml.push_str(&body(leaf, Some(k), keys)?);
        if !leaf.attributes.is_empty() {
            toml.push_str(&format!("\n[{}.attributes]\n", text::key(k)));
            toml.push_str(&attributes(leaf)?);
        }
        add_extras(leaf, dir, k, files);
    }
    files.insert(join(dir, PROPS_FILE), toml.into_bytes());
    for child in node.children.iter().filter(|c| !c.children.is_empty()) {
        let k = keys.key(child.id);
        add_extras(child, dir, k, files);
        write_folder(child, &join(dir, k), Some(k), keys, files)?;
    }
    Ok(())
}

fn add_extras(node: &Node, dir: &str, stem: &str, files: &mut Files) {
    if let Some(source) = &node.source {
        files.insert(
            join(dir, &format!("{stem}.luau")),
            source.clone().into_bytes(),
        );
    }
    if let Some(model) = &node.model {
        files.insert(join(dir, &format!("{stem}.rbxm")), model.clone());
    }
}

/// `class`, `name` when the disk name isn't the real one, `tags`, then properties by name.
fn body(node: &Node, key: Option<&str>, keys: &KeyMap) -> Result<String> {
    let mut out = format!("class = {}\n", text::string(&node.class));
    if key != Some(node.name.as_str()) {
        out.push_str(&format!("name = {}\n", text::string(&node.name)));
    }
    if !node.tags.is_empty() {
        let mut tags = node.tags.clone();
        tags.sort();
        out.push_str(&format!(
            "tags = [{}]\n",
            tags.iter()
                .map(|t| text::string(t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let refs = |t: &RefTarget| keys.reference(t);
    for (k, v) in &node.properties {
        if names::RESERVED.contains(&k.as_str()) {
            bail!(
                "{} {:?} has a property named {k:?}, which forge's files use for themselves",
                node.class,
                node.name
            );
        }
        let value = v
            .to_toml(&refs)
            .with_context(|| format!("{} {:?}: {k}", node.class, node.name))?;
        out.push_str(&format!("{} = {value}\n", text::key(k)));
    }
    Ok(out)
}

fn attributes(node: &Node) -> Result<String> {
    let mut out = String::new();
    for (k, v) in &node.attributes {
        out.push_str(&format!(
            "{} = {}\n",
            text::key(k),
            v.attribute_to_toml()
                .with_context(|| format!("attribute {k}"))?
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// Reading

/// The tree in `files`. Every node gets the disk name it was read from (`Node::key`) and a fresh id.
pub fn read(files: &Files, schema: &Schema) -> Result<Node> {
    let mut ids = 0u32;
    let mut root = read_folder(files, "", None, schema, &mut ids)?;
    let mut paths = HashMap::new();
    index(&root, &mut Vec::new(), &mut paths);
    resolve(&mut root, &paths, "")?;
    Ok(root)
}

/// What a folder holds directly: sub-folders, and files other than `_.toml`.
fn listing(files: &Files, dir: &str) -> (Vec<String>, Vec<String>) {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut folders = Vec::new();
    let mut own = Vec::new();
    for path in files.keys().filter_map(|p| p.strip_prefix(&prefix)) {
        match path.split_once('/') {
            Some((folder, _)) => {
                if folders.last().map(String::as_str) != Some(folder) {
                    folders.push(folder.to_string());
                }
            }
            None => own.push(path.to_string()),
        }
    }
    folders.dedup();
    (folders, own)
}

fn read_folder(
    files: &Files,
    dir: &str,
    key: Option<&str>,
    schema: &Schema,
    ids: &mut u32,
) -> Result<Node> {
    let props_path = join(dir, PROPS_FILE);
    let where_ = || {
        if dir.is_empty() {
            PROPS_FILE.to_string()
        } else {
            props_path.clone()
        }
    };
    let bytes = files
        .get(&props_path)
        .ok_or_else(|| anyhow!("{} is missing: every folder needs one", where_()))?;
    let table: toml::Table = std::str::from_utf8(bytes)
        .map_err(|_| anyhow!("isn't UTF-8"))
        .and_then(|t| t.parse().map_err(|e| anyhow!("{e}")))
        .with_context(where_)?;
    let (mut node, leaves) = read_body(&table, key, schema, true).with_context(where_)?;
    node.id = next(ids);
    let (folders, own) = listing(files, dir);
    for (k, t) in leaves {
        let (mut leaf, _) = read_body(&t, Some(&k), schema, false)
            .with_context(|| format!("{}: [{k}]", where_()))?;
        leaf.id = next(ids);
        node.children.push(leaf);
    }
    for f in &folders {
        if f.starts_with('.') {
            continue;
        }
        if node.children.iter().any(|c| c.key.as_deref() == Some(f)) {
            bail!("{} is both a section in {} and a folder", f, where_());
        }
        node.children
            .push(read_folder(files, &join(dir, f), Some(f), schema, ids)?);
    }
    for file in own
        .iter()
        .filter(|f| *f != PROPS_FILE && !f.starts_with('.'))
    {
        let path = join(dir, file);
        let (stem, ext) = file
            .rsplit_once('.')
            .ok_or_else(|| anyhow!("{path} isn't a file forge writes"))?;
        let target = if stem == "_" && key.is_none() {
            &mut node
        } else {
            node.children
                .iter_mut()
                .find(|c| c.key.as_deref() == Some(stem))
                .ok_or_else(|| {
                    anyhow!(
                        "{path} has no instance {stem:?} in {} to belong to",
                        where_()
                    )
                })?
        };
        let bytes = files[&path].clone();
        match ext {
            "luau" => {
                target.source =
                    Some(String::from_utf8(bytes).map_err(|_| anyhow!("{path} isn't UTF-8"))?)
            }
            "rbxm" => target.model = Some(bytes),
            _ => bail!("{path} isn't a file forge writes"),
        }
    }
    node.children.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(node)
}

fn next(ids: &mut u32) -> u32 {
    *ids += 1;
    *ids
}

/// One instance's table. In a folder's `_.toml` (top), a table that isn't a property is a leaf child.
fn read_body(
    table: &toml::Table,
    key: Option<&str>,
    schema: &Schema,
    top: bool,
) -> Result<(Node, Vec<(String, toml::Table)>)> {
    let class = table
        .get("class")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("needs `class = \"...\"`"))?
        .to_string();
    if !schema.knows(&class) {
        bail!("class {class:?} isn't in the schema");
    }
    let name = match (table.get("name"), key) {
        (Some(n), _) => n
            .as_str()
            .ok_or_else(|| anyhow!("`name` must be a string"))?
            .to_string(),
        (None, Some(k)) => k.to_string(),
        (None, None) => bail!("the tree's top `_.toml` needs `name = \"...\"`"),
    };
    let mut node = Node {
        class: class.clone(),
        name,
        key: key.map(str::to_string),
        ..Default::default()
    };
    let mut leaves = Vec::new();
    for (k, v) in table {
        match k.as_str() {
            "class" | "name" => {}
            "tags" => {
                node.tags = v
                    .as_array()
                    .ok_or_else(|| anyhow!("`tags` must be a list"))?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| anyhow!("a tag must be a string"))
                    })
                    .collect::<Result<_>>()?;
            }
            "attributes" => {
                let t = v
                    .as_table()
                    .ok_or_else(|| anyhow!("`attributes` must be a table"))?;
                for (ak, av) in t {
                    node.attributes.insert(
                        ak.clone(),
                        Value::attribute_from_toml(av)
                            .with_context(|| format!("attribute {ak}"))?,
                    );
                }
            }
            // A table with `class` is always a child, whatever the schema says. A later Studio can add a property
            // with the same name as an existing child, and old files must still read the same way.
            _ if v.as_table().is_some_and(|t| t.contains_key("class")) => match v.as_table() {
                Some(t) if top => leaves.push((k.clone(), t.clone())),
                _ => bail!(
                    "{k:?} is a child inside a child's section; an instance with children is a folder"
                ),
            },
            _ => match schema.property(&class, k) {
                Some(ty) => {
                    node.properties.insert(
                        k.clone(),
                        Value::from_toml(v, ty).with_context(|| k.to_string())?,
                    );
                }
                None => bail!("{k:?} isn't a property of {class}"),
            },
        }
    }
    Ok((node, leaves))
}

fn index(node: &Node, path: &mut Vec<String>, paths: &mut HashMap<String, u32>) {
    paths.insert(path.join("/"), node.id);
    for c in &node.children {
        path.push(c.key.clone().unwrap_or_default());
        index(c, path, paths);
        path.pop();
    }
}

/// Turns every `@path` read from disk into the instance it names.
fn resolve(node: &mut Node, paths: &HashMap<String, u32>, at: &str) -> Result<()> {
    for (k, v) in node.properties.iter_mut() {
        if let Value::Ref(RefTarget::Path(p)) = v {
            let rest = &p[1..];
            *v = if let Some(abs) = rest.strip_prefix('/') {
                Value::Ref(RefTarget::Outside(
                    abs.split('/').map(names::outside_back).collect(),
                ))
            } else {
                let id = paths
                    .get(rest)
                    .ok_or_else(|| anyhow!("{at}: {k} = {p:?} points at nothing in this tree"))?;
                Value::Ref(RefTarget::Node(*id))
            };
        }
    }
    for c in node.children.iter_mut() {
        let child_at = join(at, c.key.as_deref().unwrap_or(""));
        resolve(c, paths, &child_at)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Disk

/// Reads every file under `root` (dot-files and dot-folders skipped).
pub fn read_dir(root: &Path) -> Result<Files> {
    let mut files = Files::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("can't read {}", dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = join(&rel, &name);
            if entry.file_type()?.is_dir() {
                stack.push((entry.path(), path));
            } else {
                files.insert(path, std::fs::read(entry.path())?);
            }
        }
    }
    Ok(files)
}

/// Writes `files` under `root`. Files already there and not in `files` are left alone; removing them is up to the
/// command doing the writing.
pub fn write_files(root: &Path, files: &Files) -> Result<()> {
    for (rel, bytes) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes).with_context(|| format!("can't write {}", path.display()))?;
    }
    Ok(())
}
