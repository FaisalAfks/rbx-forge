//! Packages trees read the way Rojo reads them, on folders made for the tests in the shape `wally install` leaves
//! (never game data): link modules, `_Index` entries that are small Rojo projects, init files, files Rojo skips and
//! files forge refuses. The tree read has to line up with Studio's copy of what Rojo made, so a push plans nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use forge_sync::commands::disk_tree;
use forge_sync::schema::{Class, Schema};
use forge_sync::tree::Node;
use forge_sync::trees::Kind;
use forge_sync::{layout, packages, push};

fn schema() -> Schema {
    let mut s = Schema::default();
    for class in [
        "Instance",
        "Folder",
        "ModuleScript",
        "Script",
        "LocalScript",
    ] {
        s.classes.insert(
            class.to_string(),
            Class {
                superclass: (class != "Instance").then(|| "Instance".to_string()),
                ..Default::default()
            },
        );
    }
    s
}

/// A fresh folder for one test, with `files` written into it.
fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-packages-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// A Wally install with two packages: one whose project names it, one that leaves the name to its folder.
fn install(test: &str) -> PathBuf {
    folder(
        test,
        &[
            (
                "Ecs.lua",
                "return require(script.Parent._Index[\"scope_ecs@1.0.0\"][\"ecs\"])\n",
            ),
            (
                "Ui.lua",
                "return require(script.Parent._Index[\"scope_ui@2.0.0\"][\"ui\"])\n",
            ),
            (
                "_Index/scope_ecs@1.0.0/ecs/default.project.json",
                r#"{ "name": "ecs", "tree": { "$path": "lib" } }"#,
            ),
            ("_Index/scope_ecs@1.0.0/ecs/lib/init.luau", "return 'ecs'"),
            ("_Index/scope_ecs@1.0.0/ecs/lib/World.luau", "return {}"),
            (
                "_Index/scope_ecs@1.0.0/ecs/lib/World.spec.luau",
                "return nil",
            ),
            (
                "_Index/scope_ecs@1.0.0/ecs/lib/hooks/useEvent.luau",
                "return 1",
            ),
            (
                "_Index/scope_ecs@1.0.0/ecs/lib/notes.md",
                "skipped, as Rojo does",
            ),
            // Beside the project file, so not in the package.
            (
                "_Index/scope_ecs@1.0.0/ecs/tests.server.luau",
                "print('tests')",
            ),
            ("_Index/scope_ecs@1.0.0/ecs/wally.toml", "[package]"),
            (
                "_Index/scope_ecs@1.0.0/ecs/test.project.json",
                r#"{ "name": "x", "tree": { "$className": "DataModel" } }"#,
            ),
            (
                "_Index/scope_ui@2.0.0/ui/default.project.json",
                r#"{ "tree": { "$path": "src" } }"#,
            ),
            ("_Index/scope_ui@2.0.0/ui/src/init.lua", "return 'ui'"),
            ("_Index/scope_ui@2.0.0/ui/src/Story.client.lua", "print(1)"),
            ("_Index/scope_ui@2.0.0/ui/src/Boot.server.luau", "print(2)"),
        ],
    )
}

fn child<'a>(node: &'a Node, name: &str) -> &'a Node {
    node.children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{} has no {name}", node.name))
}

fn error(dir: &Path) -> String {
    format!("{:#}", packages::read(dir, "Packages").unwrap_err())
}

#[test]
fn reads_an_install_the_way_rojo_does() {
    let dir = install("rojo");
    let top = packages::read(&dir, "Packages").unwrap();
    assert_eq!(
        (top.class.as_str(), top.name.as_str()),
        ("Folder", "Packages")
    );
    assert_eq!(child(&top, "Ecs").class, "ModuleScript");
    let index = child(&top, "_Index");
    assert_eq!(index.class, "Folder");

    let ecs = child(child(index, "scope_ecs@1.0.0"), "ecs");
    assert_eq!(ecs.class, "ModuleScript");
    assert_eq!(ecs.source.as_deref(), Some("return 'ecs'"));
    let mut names: Vec<&str> = ecs.children.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    // Only what the project's `$path` holds: no tests script, no wally.toml, no notes.
    assert_eq!(names, ["World", "World.spec", "hooks"]);
    assert_eq!(child(ecs, "hooks").class, "Folder");
    assert_eq!(child(child(ecs, "hooks"), "useEvent").class, "ModuleScript");

    // A project without `name` is named by its folder (Rojo 7.4.1 and later).
    let ui = child(child(index, "scope_ui@2.0.0"), "ui");
    assert_eq!(ui.class, "ModuleScript");
    assert_eq!(child(ui, "Story").class, "LocalScript");
    assert_eq!(child(ui, "Boot").class, "Script");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_project_name_wins_over_its_folder() {
    let dir = folder(
        "named",
        &[
            (
                "pkg/default.project.json",
                r#"{ "name": "Other", "tree": { "$path": "src" } }"#,
            ),
            ("pkg/src/init.luau", "return 1"),
        ],
    );
    let top = packages::read(&dir, "Packages").unwrap();
    assert_eq!(top.children[0].name, "Other");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_project_can_point_at_one_file() {
    let dir = folder(
        "file",
        &[
            (
                "pkg/default.project.json",
                r#"{ "name": "pkg", "tree": { "$path": "src/main.lua" } }"#,
            ),
            ("pkg/src/main.lua", "return 'main'"),
        ],
    );
    let top = packages::read(&dir, "Packages").unwrap();
    let pkg = &top.children[0];
    assert_eq!(
        (pkg.class.as_str(), pkg.name.as_str()),
        ("ModuleScript", "pkg")
    );
    assert_eq!(pkg.source.as_deref(), Some("return 'main'"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_rojo_would_make_other_instances_from_are_refused() {
    for (file, text) in [
        ("lib/data.json", "{}"),
        ("lib/config.toml", ""),
        ("lib/notes.txt", ""),
        ("lib/model.rbxm", ""),
        ("lib/init.meta.json", "{}"),
        ("lib/Tool.plugin.lua", ""),
    ] {
        let dir = folder(
            "refused",
            &[
                (
                    "pkg/default.project.json",
                    r#"{ "tree": { "$path": "lib" } }"#,
                ),
                ("pkg/lib/init.luau", "return 1"),
                (file, text),
            ],
        );
        // The extra file sits beside `pkg`, in the tree's own folders.
        let moved = dir.join("pkg").join(file);
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(dir.join(file), &moved).unwrap();
        let e = error(&dir);
        assert!(e.contains("forge reads only .lua and .luau"), "{file}: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn project_files_that_need_more_than_path_are_refused() {
    for (project, says) in [
        (
            r#"{ "tree": { "$path": "lib", "$className": "Folder" } }"#,
            "only `$path`",
        ),
        (
            r#"{ "tree": { "$path": "lib", "Child": { "$path": "x" } } }"#,
            "only `$path`",
        ),
        (
            r#"{ "globIgnorePaths": [], "tree": { "$path": "lib" } }"#,
            "globIgnorePaths",
        ),
        (
            r#"{ "tree": { "$path": "../../elsewhere" } }"#,
            "out of the packages folder",
        ),
        (r#"{ "tree": { "$path": "." } }"#, "leads back to itself"),
        (r#"{ "tree": { "$path": "gone" } }"#, "isn't there"),
    ] {
        let dir = folder(
            "project",
            &[
                ("pkg/default.project.json", project),
                ("pkg/lib/init.luau", "return 1"),
            ],
        );
        let e = error(&dir);
        assert!(e.contains(says), "{project}: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_folder_with_two_init_files_is_refused() {
    let dir = folder(
        "inits",
        &[("pkg/init.lua", "return 1"), ("pkg/init.luau", "return 2")],
    );
    assert!(error(&dir).contains("2 init files"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_folder_says_to_install() {
    let dir = std::env::temp_dir().join("forge-packages-never-made");
    assert!(error(&dir).contains("wally install"));
}

/// Studio's copy of what Rojo made from `install`, the way the plugin sends it: ids of its own, no disk names.
fn rojo_made() -> Node {
    let mut ids = 1000;
    let mut n = |class: &str, name: &str, source: Option<&str>, children: Vec<Node>| {
        ids += 1;
        Node {
            id: ids,
            class: class.into(),
            name: name.into(),
            source: source.map(str::to_string),
            children,
            ..Default::default()
        }
    };
    let world = n("ModuleScript", "World", Some("return {}"), vec![]);
    let spec = n("ModuleScript", "World.spec", Some("return nil"), vec![]);
    let use_event = n("ModuleScript", "useEvent", Some("return 1"), vec![]);
    let hooks = n("Folder", "hooks", None, vec![use_event]);
    let ecs = n(
        "ModuleScript",
        "ecs",
        Some("return 'ecs'"),
        vec![hooks, spec, world],
    );
    let ecs_at = n("Folder", "scope_ecs@1.0.0", None, vec![ecs]);
    let boot = n("Script", "Boot", Some("print(2)"), vec![]);
    let story = n("LocalScript", "Story", Some("print(1)"), vec![]);
    let ui = n("ModuleScript", "ui", Some("return 'ui'"), vec![story, boot]);
    let ui_at = n("Folder", "scope_ui@2.0.0", None, vec![ui]);
    let index = n("Folder", "_Index", None, vec![ui_at, ecs_at]);
    let link_ui = n(
        "ModuleScript",
        "Ui",
        Some("return require(script.Parent._Index[\"scope_ui@2.0.0\"][\"ui\"])\n"),
        vec![],
    );
    let link_ecs = n(
        "ModuleScript",
        "Ecs",
        Some("return require(script.Parent._Index[\"scope_ecs@1.0.0\"][\"ecs\"])\n"),
        vec![],
    );
    n("Folder", "Packages", None, vec![index, link_ui, link_ecs])
}

#[test]
fn a_push_onto_what_rojo_made_changes_nothing() {
    let dir = install("push");
    let s = schema();
    let on_disk = disk_tree(Kind::Packages, "Packages", &dir, &s).unwrap();
    let studio = rojo_made();
    let paths: HashMap<u32, String> = layout::disk_paths(&studio, &s);
    let plan = push::plan(&studio, paths, &on_disk).unwrap();
    assert!(plan.is_empty(), "{:#?}", plan.ops);

    // And an edit to a package's file is one change.
    std::fs::write(
        dir.join("_Index/scope_ecs@1.0.0/ecs/lib/World.luau"),
        "return { v = 2 }",
    )
    .unwrap();
    let on_disk = disk_tree(Kind::Packages, "Packages", &dir, &s).unwrap();
    let plan = push::plan(&studio, layout::disk_paths(&studio, &s), &on_disk).unwrap();
    assert_eq!((plan.changed, plan.added, plan.removed), (1, 0, 0));
    let _ = std::fs::remove_dir_all(&dir);
}
