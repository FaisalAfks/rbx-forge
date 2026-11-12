//! `forge sync sourcemap` on the fixture game in `fixtures/sourcemap-game` (made for the tests, never game data):
//! trees under their Studio paths, file paths from the game's root, packages read the way Rojo reads them, and only
//! scripts and what leads to them unless non-scripts are asked for. CI also checks the map with luau-lsp.

use std::path::PathBuf;

use forge_sync::sourcemap::{self, Entry};
use forge_sync::trees::{self, Game};

fn game() -> Game {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sourcemap-game/forge.toml");
    let core = forge_core::game::load(&file, &[&forge_sync::tool::SYNC]).unwrap();
    trees::read(&core).unwrap()
}

fn at<'a>(entry: &'a Entry, path: &[&str]) -> &'a Entry {
    path.iter().fold(entry, |e, name| {
        e.children
            .iter()
            .find(|c| c.name == *name)
            .unwrap_or_else(|| panic!("{} has no {name}", e.name))
    })
}

#[test]
fn trees_land_under_their_studio_paths() {
    let map = sourcemap::build(&game(), false).unwrap();
    assert_eq!(
        (map.class.as_str(), map.files.as_slice()),
        ("DataModel", &["forge.toml".to_string()][..])
    );
    let counter = at(&map, &["ReplicatedStorage", "Shared", "Util", "Counter"]);
    assert_eq!(counter.class, "ModuleScript");
    assert_eq!(counter.files, ["shared/Util/Counter.luau"]);

    // StarterPlayerScripts is a container Roblox makes, so it keeps its class.
    let scripts = at(&map, &["StarterPlayer", "StarterPlayerScripts"]);
    assert_eq!(scripts.class, "StarterPlayerScripts");
    let main = at(scripts, &["Client", "Main"]);
    assert_eq!(
        (main.class.as_str(), main.files.as_slice()),
        ("LocalScript", &["client/Main.client.luau".to_string()][..])
    );

    let lib = at(
        &map,
        &[
            "ReplicatedStorage",
            "Packages",
            "_Index",
            "x_lib@1.0.0",
            "lib",
        ],
    );
    assert_eq!(lib.files, ["Packages/_Index/x_lib@1.0.0/lib/src/init.luau"]);
    assert_eq!(
        at(&map, &["ReplicatedStorage", "Packages", "Lib"]).files,
        ["Packages/Lib.lua"]
    );
}

#[test]
fn non_scripts_are_left_out_unless_asked_for() {
    let shared = |non_scripts| {
        let map = sourcemap::build(&game(), non_scripts).unwrap();
        at(&map, &["ReplicatedStorage", "Shared"])
            .children
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(shared(false), ["Util"]);
    assert_eq!(shared(true), ["Settings", "Util"]);
}

#[test]
fn the_json_is_rojos_shape() {
    let map = sourcemap::build(&game(), false).unwrap();
    let json = serde_json::to_value(&map).unwrap();
    assert_eq!(json["className"], "DataModel");
    let first = &json["children"][0];
    assert!(first.get("filePaths").is_none(), "no empty lists: {first}");
    assert!(first["children"].is_array());
}
