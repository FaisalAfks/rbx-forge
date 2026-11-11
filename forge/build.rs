//! Builds the plugin's Luau into the binary, so `forge plugin install` works wherever forge is installed. Collects
//! `plugin/**.luau` and `tools/<name>/studio/**.luau` (decision 0015) into `$OUT_DIR/plugin_files.rs`, as a list of
//! (path from the repo's root, text).

use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("forge/ is inside the repo");
    // Whole folders, so a new tool's studio/ is picked up; a path that doesn't exist yet would rerun every build.
    for dir in ["plugin", "tools"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }

    let mut files = Vec::new();
    collect(root, &root.join("plugin"), &mut files);
    if let Ok(tools) = std::fs::read_dir(root.join("tools")) {
        for tool in tools.flatten() {
            collect(root, &tool.path().join("studio"), &mut files);
        }
    }
    files.sort();

    let mut out = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for (rel, abs) in &files {
        out.push_str(&format!("    ({rel:?}, include_str!({abs:?})),\n"));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("plugin_files.rs");
    std::fs::write(dest, out).expect("can write to OUT_DIR");
}

/// Every `.luau` file under `dir`, as (path from `root` with `/`, absolute path).
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.extension().is_some_and(|e| e == "luau") {
            let rel = path.strip_prefix(root).expect("under the root");
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((rel, path.display().to_string()));
        }
    }
}
