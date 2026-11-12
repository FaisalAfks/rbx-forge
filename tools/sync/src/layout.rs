//! Layout C (decisions 0008 and 0020): a tree as files and back. An instance with children is a folder with a
//! `_.toml` (its own class, properties, attributes and tags, plus one section per child without children). A plain
//! Folder needs no `_.toml` (decision 0022).
//!
//! Scripts are named the way Luau tools read them: `Name.luau` is a ModuleScript, `Name.server.luau` a Script and
//! `Name.client.luau` a LocalScript, in the parent's folder; a script with children keeps its text in its own folder
//! as `init.luau` (`init.server.luau`, `init.client.luau`). A script gets a section, or its folder a `_.toml`, only
//! when there's more to say than its file name does; a section that is written always names its class. A MeshPart's
//! model is `<name>.rbxm` in the parent's folder (the root's is `_.rbxm` in its own). Every file is written whole, in
//! one fixed form (decision 0015), so the same tree is always the same bytes. `docs/sync/format.md` is the reference.

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

/// The stem of a script's own file in its folder.
const INIT: &str = "init";

/// Each script class and the suffix its file name carries before `.luau`.
const SCRIPT_SUFFIXES: [(&str, &str); 3] = [
    ("ModuleScript", ""),
    ("Script", ".server"),
    ("LocalScript", ".client"),
];

/// What a file in v0.1's layout gets, pointing at the fix.
const OLD_LAYOUT: &str = "forge 0.1 named script files differently: run `forge sync pull` to rewrite them (decision 0020)";

/// The suffix a script class's file name carries, if forge writes that class's text.
fn script_suffix(class: &str) -> Option<&'static str> {
    SCRIPT_SUFFIXES
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, s)| *s)
}

/// A `.luau` file's stem split into its instance's disk name and the class the name gives (`Foo.server` is a Script
/// with disk name `Foo`). Names that end in `.server` or `.client` are escaped (`names::base`), so this can't misread.
fn script_file(stem: &str) -> (&str, &'static str) {
    for (class, suffix) in SCRIPT_SUFFIXES.iter().skip(1) {
        if let Some(base) = stem.strip_suffix(suffix) {
            return (base, class);
        }
    }
    (stem, SCRIPT_SUFFIXES[0].0)
}

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
    // A script's own text is its folder's `init` file. The root's model is its folder's `_.rbxm`, since it has no
    // parent folder in the tree.
    add_source(node, dir, INIT, files)?;
    if key.is_none() {
        add_model(node, dir, "_", files);
    }
    // A folder needs a `_.toml` only when its name and files don't say everything (a plain Folder, or a script whose
    // file name gives its class). The root always has one, with its name, and it marks the folder as forge's.
    let mut needed = key.is_none() || !(plain_script(node, key) || plain_folder(node, key));
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
        add_source(leaf, dir, k, files)?;
        add_model(leaf, dir, k, files);
        if plain_script(leaf, Some(k)) {
            continue;
        }
        needed = true;
        toml.push_str(&format!("\n[{}]\n", text::key(k)));
        toml.push_str(&body(leaf, Some(k), keys)?);
        if !leaf.attributes.is_empty() {
            toml.push_str(&format!("\n[{}.attributes]\n", text::key(k)));
            toml.push_str(&attributes(leaf)?);
        }
    }
    if needed {
        files.insert(join(dir, PROPS_FILE), toml.into_bytes());
    }
    for child in node.children.iter().filter(|c| !c.children.is_empty()) {
        let k = keys.key(child.id);
        add_model(child, dir, k, files);
        write_folder(child, &join(dir, k), Some(k), keys, files)?;
    }
    Ok(())
}

/// A script whose file name says all there is: its class and its name, and nothing else set.
fn plain_script(node: &Node, key: Option<&str>) -> bool {
    node.source.is_some()
        && script_suffix(&node.class).is_some()
        && key == Some(node.name.as_str())
        && node.properties.is_empty()
        && node.attributes.is_empty()
        && node.tags.is_empty()
}

/// A Folder with children and nothing else to say: no text, its real name, no properties, attributes or tags.
fn plain_folder(node: &Node, key: Option<&str>) -> bool {
    node.class == "Folder"
        && node.source.is_none()
        && node.model.is_none()
        && key == Some(node.name.as_str())
        && node.properties.is_empty()
        && node.attributes.is_empty()
        && node.tags.is_empty()
}

/// A script's text, as `<stem><suffix>.luau` in `dir`.
fn add_source(node: &Node, dir: &str, stem: &str, files: &mut Files) -> Result<()> {
    let Some(source) = &node.source else {
        return Ok(());
    };
    let suffix = script_suffix(&node.class).ok_or_else(|| {
        anyhow!(
            "{} {:?} has script text, but forge writes text only for Script, LocalScript and ModuleScript",
            node.class,
            node.name
        )
    })?;
    files.insert(
        join(dir, &format!("{stem}{suffix}.luau")),
        source.clone().into_bytes(),
    );
    Ok(())
}

fn add_model(node: &Node, dir: &str, stem: &str, files: &mut Files) {
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
    let at = |file: &str| join(dir, file);
    let (folders, own) = listing(files, dir);
    if key.is_none() && own.iter().any(|f| f == "_.luau") {
        bail!(
            "{} holds the tree's script text: {OLD_LAYOUT}",
            at("_.luau")
        );
    }
    // The folder's own script, if it's a script: `init.luau`, `init.server.luau` or `init.client.luau`.
    let mut init: Option<(&String, &'static str)> = None;
    for file in &own {
        let Some(stem) = file.strip_suffix(".luau") else {
            continue;
        };
        let (base, class) = script_file(stem);
        if base == INIT {
            if let Some((other, _)) = init {
                bail!(
                    "{} and {} are both the folder's own script",
                    at(other),
                    at(file)
                );
            }
            init = Some((file, class));
        }
    }

    let mut node = match files.get(&props_path) {
        Some(bytes) => {
            let table: toml::Table = std::str::from_utf8(bytes)
                .map_err(|_| anyhow!("isn't UTF-8"))
                .and_then(|t| t.parse().map_err(|e| anyhow!("{e}")))
                .with_context(where_)?;
            let (mut node, leaves) = read_body(&table, key, schema, true).with_context(where_)?;
            node.id = next(ids);
            if let Some((file, class)) = init
                && node.class != class
            {
                bail!(
                    "{} says class {}, but {} is a {class}'s file",
                    where_(),
                    node.class,
                    at(file)
                );
            }
            for (k, t) in leaves {
                let (mut leaf, _) = read_body(&t, Some(&k), schema, false)
                    .with_context(|| format!("{}: [{k}]", where_()))?;
                leaf.id = next(ids);
                node.children.push(leaf);
            }
            node
        }
        // A folder with nothing more to say than its name and files has no `_.toml`: a script's folder takes its
        // init file's class, and any other is a Folder (decision 0022).
        None => match key {
            Some(k) => Node {
                class: init.map_or("Folder", |(_, class)| class).to_string(),
                name: k.to_string(),
                key: Some(k.to_string()),
                id: next(ids),
                ..Default::default()
            },
            None => bail!("{} is missing: the tree's top needs one", where_()),
        },
    };

    let mut folder_keys = Vec::new();
    for f in &folders {
        if f.starts_with('.') {
            continue;
        }
        if node.children.iter().any(|c| c.key.as_deref() == Some(f)) {
            bail!("{} is both a section in {} and a folder", f, where_());
        }
        node.children
            .push(read_folder(files, &join(dir, f), Some(f), schema, ids)?);
        folder_keys.push(f.as_str());
    }
    for file in own
        .iter()
        .filter(|f| *f != PROPS_FILE && !f.starts_with('.'))
    {
        let path = at(file);
        let bytes = files[&path].clone();
        if let Some(stem) = file.strip_suffix(".luau") {
            let text = String::from_utf8(bytes).map_err(|_| anyhow!("{path} isn't UTF-8"))?;
            let (base, class) = script_file(stem);
            if base == INIT {
                node.source = Some(text);
                continue;
            }
            if folder_keys.contains(&base) {
                bail!(
                    "{path} is the text of {base}, which has a folder; its text belongs in that folder's init file: \
                     {OLD_LAYOUT}"
                );
            }
            match node
                .children
                .iter_mut()
                .find(|c| c.key.as_deref() == Some(base))
            {
                Some(c) if c.source.is_some() => {
                    bail!("{path}: {base} has two script files")
                }
                Some(c) if c.class != class => bail!(
                    "{path} is a {class}'s file, but its section in {} says class {}: {OLD_LAYOUT}",
                    where_(),
                    c.class
                ),
                Some(c) => c.source = Some(text),
                // A plain script is just its file: the name gives its class and its own name.
                None => node.children.push(Node {
                    class: class.to_string(),
                    name: base.to_string(),
                    key: Some(base.to_string()),
                    id: next(ids),
                    source: Some(text),
                    ..Default::default()
                }),
            }
            continue;
        }
        let Some(stem) = file.strip_suffix(".rbxm") else {
            bail!("{path} isn't a file forge writes");
        };
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
        target.model = Some(bytes);
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
