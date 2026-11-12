//! `forge plugin`: packs the plugin's Luau (built into the binary by `build.rs`) into one `forge.rbxmx` and installs
//! it in Studio's Plugins folder (decision 0015), or writes a sourcemap so luau-lsp can follow its `require`s.
//!
//! How files become instances:
//! - `plugin/main.luau` is the plugin's Script, named `forge`; the settings the plugin needs are its attributes.
//! - Every other file in `plugin/` is a ModuleScript under it, named after the file; folders become Folders.
//! - Each `tools/<name>/studio/` becomes the Folder `tools/<name>` under it, packed the same way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rbx_dom_weak::types::{Attributes, Enum, Variant};
use rbx_dom_weak::{InstanceBuilder, WeakDom};
use serde::Serialize;

use forge_core::protocol::PROTOCOL;
use forge_core::settings::Settings;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/plugin_files.rs"));
}

/// The plugin's files, as (path from the repo's root, text).
pub fn files() -> &'static [(&'static str, &'static str)] {
    embedded::FILES
}

pub const FILE_NAME: &str = "forge.rbxmx";
const MAIN: &str = "plugin/main.luau";

/// One instance of the packed plugin, before it's turned into a model file or a sourcemap.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub class: &'static str,
    /// The file it comes from, for scripts.
    pub path: Option<String>,
    pub source: Option<String>,
    pub children: BTreeMap<String, Node>,
}

impl Node {
    fn folder(name: &str) -> Node {
        Node {
            name: name.to_string(),
            class: "Folder",
            path: None,
            source: None,
            children: BTreeMap::new(),
        }
    }
}

/// Lays the files out as the plugin's instance tree.
pub fn tree(files: &[(&str, &str)]) -> Result<Node> {
    let Some((_, main)) = files.iter().find(|(path, _)| *path == MAIN) else {
        bail!("the plugin has no {MAIN}");
    };
    let mut root = Node {
        name: "forge".into(),
        class: "Script",
        path: Some(MAIN.into()),
        source: Some(main.to_string()),
        children: BTreeMap::new(),
    };
    for (path, text) in files {
        let mut steps: Vec<&str> = path.split('/').collect();
        let place: Vec<&str> = match steps.as_slice() {
            ["plugin", "main.luau"] => continue,
            ["plugin", ..] => steps.split_off(1),
            ["tools", tool, "studio", ..] => {
                let mut place = vec!["tools", *tool];
                place.extend_from_slice(&steps[3..]);
                place
            }
            _ => bail!("{path} isn't part of the plugin"),
        };
        add(&mut root, &place, path, text)?;
    }
    Ok(root)
}

fn add(root: &mut Node, place: &[&str], path: &str, text: &str) -> Result<()> {
    let (file, folders) = place.split_last().expect("a file has a name");
    let mut node = root;
    for folder in folders {
        node = node
            .children
            .entry(folder.to_string())
            .or_insert_with(|| Node::folder(folder));
        if node.class != "Folder" {
            bail!("{path}: {folder} is both a module and a folder in the plugin");
        }
    }
    let name = file
        .strip_suffix(".luau")
        .expect("only .luau files are collected");
    let module = Node {
        name: name.to_string(),
        class: "ModuleScript",
        path: Some(path.to_string()),
        source: Some(text.to_string()),
        children: BTreeMap::new(),
    };
    if node.children.insert(name.to_string(), module).is_some() {
        bail!("{path}: {name} is both a module and a folder in the plugin");
    }
    Ok(())
}

/// The plugin as a `.rbxmx` file, with the settings it needs as attributes on its Script.
pub fn rbxmx(root: &Node, settings: &Settings) -> Result<Vec<u8>> {
    let attributes = Attributes::new()
        .with("version", env!("CARGO_PKG_VERSION"))
        .with("protocol", f64::from(PROTOCOL))
        .with("port", f64::from(settings.server.port))
        // Whole milliseconds well under 2^53, so the conversion is exact.
        .with("retryMs", settings.plugin.retry_ms as f64)
        .with("openTimeoutMs", settings.plugin.open_timeout_ms as f64);
    let script = builder(root).with_property("Attributes", Variant::Attributes(attributes));
    let dom = WeakDom::new(InstanceBuilder::new("Folder").with_child(script));
    let mut out = Vec::new();
    rbx_xml::to_writer_default(&mut out, &dom, dom.root().children())
        .context("can't write the plugin file")?;
    Ok(out)
}

fn builder(node: &Node) -> InstanceBuilder {
    let mut b = InstanceBuilder::new(node.class).with_name(&node.name);
    if let Some(source) = &node.source {
        b = b.with_property("Source", source.as_str());
    }
    if node.class == "Script" {
        // Legacy, like Roblox's own plugin file: a plugin's Script runs where the plugin is loaded.
        b = b.with_property("RunContext", Enum::from_u32(0));
    }
    b.with_children(node.children.values().map(builder))
}

/// A Rojo-style sourcemap of the plugin, for luau-lsp.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcemapNode {
    name: String,
    class_name: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    file_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<SourcemapNode>,
}

pub fn sourcemap(node: &Node) -> SourcemapNode {
    SourcemapNode {
        name: node.name.clone(),
        class_name: node.class,
        file_paths: node.path.iter().cloned().collect(),
        children: node.children.values().map(sourcemap).collect(),
    }
}

/// Studio's Plugins folder on this PC, if Studio is installed.
pub fn plugins_folder() -> Option<PathBuf> {
    let roblox = if cfg!(target_os = "macos") {
        dirs::document_dir()?.join("Roblox")
    } else if cfg!(windows) {
        dirs::data_local_dir()?.join("Roblox")
    } else {
        return None;
    };
    roblox.is_dir().then(|| roblox.join("Plugins"))
}

/// Writes the plugin file to `out`, or into Studio's Plugins folder. Returns where it went.
pub fn install(settings: &Settings, out: Option<&Path>) -> Result<(PathBuf, usize)> {
    let root = tree(files())?;
    let bytes = rbxmx(&root, settings)?;
    let path = match out {
        Some(path) => path.to_path_buf(),
        None => {
            let folder = plugins_folder().context(
                "can't find Studio's Plugins folder on this PC (is Studio installed?). Use --out to write the file \
                 somewhere else",
            )?;
            std::fs::create_dir_all(&folder)
                .with_context(|| format!("can't make {}", folder.display()))?;
            folder.join(FILE_NAME)
        }
    };
    std::fs::write(&path, bytes).with_context(|| format!("can't write {}", path.display()))?;
    Ok((path, count(&root)))
}

/// How many scripts the plugin has.
fn count(node: &Node) -> usize {
    usize::from(node.source.is_some()) + node.children.values().map(count).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        forge_core::settings::load(None, &[]).unwrap()
    }

    #[test]
    fn the_built_in_plugin_packs() {
        let root = tree(files()).unwrap();
        assert_eq!(root.class, "Script");
        assert!(root.source.as_deref().unwrap().contains("link"));
        assert_eq!(root.children["link"].class, "ModuleScript");
    }

    #[test]
    fn files_become_modules_folders_and_tool_folders() {
        let files = [
            ("plugin/main.luau", "-- main"),
            ("plugin/link.luau", "return {}"),
            ("plugin/ui/widget.luau", "return 1"),
            ("tools/sync/studio/read.luau", "return 2"),
        ];
        let root = tree(&files).unwrap();
        assert_eq!(root.children["ui"].class, "Folder");
        assert_eq!(
            root.children["ui"].children["widget"].source.as_deref(),
            Some("return 1")
        );
        let sync = &root.children["tools"].children["sync"];
        assert_eq!(sync.class, "Folder");
        assert_eq!(
            sync.children["read"].path.as_deref(),
            Some("tools/sync/studio/read.luau")
        );
    }

    #[test]
    fn a_module_and_folder_with_one_name_is_an_error() {
        let files = [
            ("plugin/main.luau", ""),
            ("plugin/ui.luau", ""),
            ("plugin/ui/widget.luau", ""),
        ];
        let err = tree(&files).unwrap_err().to_string();
        assert!(err.contains("both a module and a folder"), "{err}");
    }

    #[test]
    fn a_plugin_without_main_is_an_error() {
        assert!(tree(&[("plugin/link.luau", "")]).is_err());
    }

    #[test]
    fn the_file_reads_back_with_its_settings_and_text() {
        let files = [
            ("plugin/main.luau", "print(\"]]> & <tags>\")"),
            ("plugin/link.luau", "return {}"),
        ];
        let mut s = settings();
        s.server.port = 35001;
        let bytes = rbxmx(&tree(&files).unwrap(), &s).unwrap();
        let dom = rbx_xml::from_reader_default(bytes.as_slice()).unwrap();
        let script = dom.get_by_ref(dom.root().children()[0]).unwrap();
        assert_eq!(script.name, "forge");
        assert_eq!(script.class, "Script");
        let Some(Variant::String(source)) = script.properties.get(&"Source".into()) else {
            panic!("no Source");
        };
        assert_eq!(source, "print(\"]]> & <tags>\")");
        let Some(Variant::Attributes(attrs)) = script.properties.get(&"Attributes".into()) else {
            panic!("no attributes");
        };
        assert_eq!(attrs.get("port"), Some(&Variant::Float64(35001.0)));
        assert_eq!(
            attrs.get("retryMs"),
            Some(&Variant::Float64(s.plugin.retry_ms as f64))
        );
        assert_eq!(
            attrs.get("protocol"),
            Some(&Variant::Float64(f64::from(PROTOCOL)))
        );
        let link = dom.get_by_ref(script.children()[0]).unwrap();
        assert_eq!(
            (link.name.as_str(), link.class.as_str()),
            ("link", "ModuleScript")
        );
    }

    #[test]
    fn the_sourcemap_names_each_file() {
        let root = tree(&[("plugin/main.luau", ""), ("plugin/link.luau", "")]).unwrap();
        let json = serde_json::to_value(sourcemap(&root)).unwrap();
        assert_eq!(json["className"], "Script");
        assert_eq!(json["filePaths"][0], "plugin/main.luau");
        assert_eq!(json["children"][0]["name"], "link");
    }
}
