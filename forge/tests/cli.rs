//! Runs the built `forge` binary the way people and agents do, and checks what it prints.

use std::path::PathBuf;
use std::process::{Command, Output};

fn forge(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .output()
        .expect("forge runs")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn prints_its_version() {
    let out = forge(&["--version"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).starts_with("forge "));
}

#[test]
fn config_shows_settings_and_the_game() {
    let dir = temp_dir("game");
    std::fs::write(
        dir.join("forge.toml"),
        "place = 42\n[[sync]]\nstudio = \"StarterGui.Hud\"\ndisk = \"game/hud\"\n",
    )
    .unwrap();
    let out = forge(&["config", "--dir", dir.to_str().unwrap()]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.contains("port = "), "{stdout}");
    assert!(stdout.contains("place = 42"), "{stdout}");
    assert!(stdout.contains("StarterGui.Hud -> game"), "{stdout}");
}

#[test]
fn a_bad_forge_toml_is_a_readable_error() {
    let dir = temp_dir("bad");
    std::fs::write(dir.join("forge.toml"), "place = 0\n").unwrap();
    let out = forge(&["config", "--dir", dir.to_str().unwrap()]);
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("forge.toml is wrong") && stderr.contains("place id"),
        "{stderr}"
    );
}
