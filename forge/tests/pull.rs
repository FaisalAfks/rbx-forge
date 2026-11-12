//! `forge sync pull` end to end, without Studio: the real `forge` binary as server and command, and a stand-in plugin
//! that answers the two requests with a small made-up tree (never game data). Studio's own side is checked in Studio.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value as Json, json};
use tungstenite::Message;

/// The link protocol this forge speaks (forge/src/protocol.rs).
const PROTOCOL: u32 = 2;

/// A script with CRLF line ends and no last newline: git would change those bytes without the `.gitattributes`.
const SCRIPT: &str = "local x = 1\r\nreturn x";

struct Server {
    dir: PathBuf,
    settings: PathBuf,
    port: u16,
}

impl Server {
    fn new(name: &str) -> Server {
        let dir = std::env::temp_dir().join(format!("forge pull {}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let settings = dir.join("settings.toml");
        std::fs::write(
            &settings,
            format!("[server]\nport = {port}\n[plugin]\nretry_ms = 100\nopen_timeout_ms = 100\n"),
        )
        .unwrap();
        Server {
            dir,
            settings,
            port,
        }
    }

    /// Runs forge with its output in pipes, with a deadline (see tests/link.rs).
    fn forge(&self, args: &[&str], cwd: &Path) -> (bool, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
            .args(args)
            .current_dir(cwd)
            .env("FORGE_SETTINGS", &self.settings)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("forge runs");
        let (tx, rx) = mpsc::channel();
        for mut pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text);
                let _ = tx.send(text);
            });
        }
        let mut text = String::new();
        for _ in 0..2 {
            text.push_str(
                &rx.recv_timeout(Duration::from_secs(60))
                    .expect("forge's output ended"),
            );
        }
        (child.wait().unwrap().success(), text)
    }

    /// A stand-in plugin on `place` that answers every request it gets, in a thread, and passes each request on.
    fn studio(&self, place: u64) -> mpsc::Receiver<Json> {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let (mut ws, _) =
            tungstenite::client(format!("ws://127.0.0.1:{}", self.port), stream).unwrap();
        let hello = json!({ "op": "hello", "role": "studio", "protocol": PROTOCOL, "plugin": env!("CARGO_PKG_VERSION"),
                            "session": "S", "place": place, "game": 9, "name": "Place1" });
        ws.send(Message::text(hello.to_string())).unwrap();
        let welcome = ws.read().unwrap().into_text().unwrap().to_string();
        assert!(welcome.contains("welcome"), "{welcome}");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(frame) = ws.read() {
                let Ok(text) = frame.into_text() else {
                    continue;
                };
                let Ok(message) = serde_json::from_str::<Json>(text.as_str()) else {
                    continue;
                };
                if message["op"] != "request" {
                    continue;
                }
                let request = message["request"].clone();
                let reply = match request["action"].as_str() {
                    Some("classes") => classes(),
                    Some("drafts") => json!({ "drafts": [["StarterGui.Hud.Main"]] }),
                    // The copy differs in one value, so the check has something to report.
                    Some("check") => {
                        let mut copy = tree();
                        copy["trees"][0]["tree"]["properties"]["Size"]["value"] =
                            json!([0, 341, 0, 66]);
                        copy
                    }
                    _ => tree(),
                };
                let answer = json!({ "op": "reply", "id": message["id"], "reply": reply });
                if ws.send(Message::text(answer.to_string())).is_err() {
                    break;
                }
                let _ = tx.send(request);
            }
        });
        rx
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.forge(&["stop"], &self.dir.clone());
    }
}

fn prop(name: &str, owner: &str, engine: &str, read: bool, write: bool) -> Json {
    json!({ "name": name, "owner": owner, "engine": engine, "serialized": true, "read": read, "write": write,
            "deprecated": false })
}

/// Studio's reflection data for the two classes in the tree, with one property plugins can't write.
fn classes() -> Json {
    json!({ "classes": {
        "Frame": [prop("Name", "Instance", "string", true, true), prop("Size", "GuiObject", "UDim2", true, true)],
        "ModuleScript": [prop("Name", "Instance", "string", true, true),
                         prop("RunContext", "BaseScript", "Enum", true, false)],
    } })
}

fn tree() -> Json {
    json!({ "trees": [ { "ms": 3, "tree": {
        "id": 1, "class": "Frame", "name": "Hud",
        "properties": { "Size": { "type": "UDim2", "value": [0, 340, 0, 66] } }, "attributes": [], "tags": [],
        "children": [ { "id": 2, "class": "ModuleScript", "name": "Main", "properties": [], "attributes": [],
                        "tags": [], "source": SCRIPT } ]
    }, "drafts": ["StarterGui.Hud.Main"] } ] })
}

fn game_repo(server: &Server) -> PathBuf {
    let repo = server.dir.join("game");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("forge.toml"),
        "place = 42\n\n[[sync]]\nstudio = \"StarterGui.Hud\"\ndisk = \"ui/hud\"\n",
    )
    .unwrap();
    repo
}

#[test]
fn pull_writes_the_tree_and_a_second_pull_changes_nothing() {
    let server = Server::new("twice");
    let repo = game_repo(&server);
    assert!(server.forge(&["status"], &repo).0);
    let requests = server.studio(42);

    let (ok, out) = server.forge(&["sync", "pull"], &repo);
    assert!(ok, "{out}");
    assert!(out.contains("StarterGui.Hud -> ui"), "{out}");
    assert!(
        out.contains("2 instances; 3 written, 0 removed, 0 unchanged"),
        "{out}"
    );
    assert!(
        out.contains("BaseScript.RunContext (Enum)"),
        "the unwritable property is reported: {out}"
    );
    assert!(out.contains("1 scripts have an uncommitted draft"), "{out}");
    assert!(out.contains("  StarterGui.Hud.Main"), "{out}");

    let classes_request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(classes_request["trees"], json!([["StarterGui", "Hud"]]));
    let read_request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        read_request["properties"]["Frame"],
        json!({ "Size": "UDim2" })
    );
    assert_eq!(read_request["classes"], json!(["Frame", "ModuleScript"]));

    let hud = repo.join("ui").join("hud");
    let props = std::fs::read_to_string(hud.join("_.toml")).unwrap();
    assert!(props.contains("class = \"Frame\""), "{props}");
    assert!(props.contains("Size = [0, 340, 0, 66]"), "{props}");
    assert_eq!(
        std::fs::read(hud.join("Main.luau")).unwrap(),
        SCRIPT.as_bytes()
    );
    assert!(hud.join(".gitattributes").exists());

    let (ok, out) = server.forge(&["sync", "pull"], &repo);
    assert!(ok, "{out}");
    assert!(out.contains("0 written, 0 removed, 3 unchanged"), "{out}");
}

#[test]
fn pull_says_when_no_studio_has_the_place() {
    let server = Server::new("nostudio");
    let repo = game_repo(&server);
    let (ok, out) = server.forge(&["sync", "pull"], &repo);
    assert!(!ok);
    assert!(out.contains("no Studio with place 42"), "{out}");
}

#[test]
fn pulled_scripts_keep_their_bytes_through_git_with_autocrlf() {
    let server = Server::new("git");
    let repo = game_repo(&server);
    let git = |dir: &Path, args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "core.autocrlf=true",
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
            ])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&repo, &["init", "-q"]) {
        return; // no git on this system
    }
    // The usual game repo setting, which the synced folder's own .gitattributes must win against.
    std::fs::write(repo.join(".gitattributes"), "* text=auto\n").unwrap();
    assert!(server.forge(&["status"], &repo).0);
    let _requests = server.studio(42);
    let (ok, out) = server.forge(&["sync", "pull"], &repo);
    assert!(ok, "{out}");
    assert!(git(&repo, &["add", "."]));
    assert!(git(&repo, &["commit", "-qm", "pull"]));

    let clone = server.dir.join("clone");
    let cloned = Command::new("git")
        .args(["-c", "core.autocrlf=true", "clone", "-q"])
        .arg(&repo)
        .arg(&clone)
        .output()
        .unwrap();
    assert!(
        cloned.status.success(),
        "{}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    let script = std::fs::read(clone.join("ui").join("hud").join("Main.luau")).unwrap();
    assert_eq!(script, SCRIPT.as_bytes(), "git changed the script's bytes");
    let props = std::fs::read(clone.join("ui").join("hud").join("_.toml")).unwrap();
    assert_eq!(
        props,
        std::fs::read(repo.join("ui").join("hud").join("_.toml")).unwrap()
    );
}

#[test]
fn sync_status_lists_drafts() {
    let server = Server::new("status");
    let repo = game_repo(&server);
    assert!(server.forge(&["status"], &repo).0);
    let _requests = server.studio(42);
    let (ok, out) = server.forge(&["sync", "status"], &repo);
    assert!(ok, "{out}");
    assert!(out.contains("place 42: linked to Studio"), "{out}");
    assert!(
        out.contains("StarterGui.Hud -> ui/hud: 1 scripts with a draft"),
        "{out}"
    );
    assert!(out.contains("  StarterGui.Hud.Main"), "{out}");
}

#[test]
fn diff_shows_what_changed_on_disk() {
    let server = Server::new("diff");
    let repo = game_repo(&server);
    assert!(server.forge(&["status"], &repo).0);
    let _requests = server.studio(42);
    let (ok, out) = server.forge(&["sync", "diff"], &repo);
    assert!(ok, "{out}");
    assert!(
        out.contains("StarterGui.Hud -> ui/hud: not pulled yet"),
        "{out}"
    );

    assert!(server.forge(&["sync", "pull"], &repo).0);
    let (ok, out) = server.forge(&["sync", "diff"], &repo);
    assert!(ok, "{out}");
    assert!(out.contains("disk and Studio match"), "{out}");

    let props = repo.join("ui").join("hud").join("_.toml");
    let text = std::fs::read_to_string(&props)
        .unwrap()
        .replace("[0, 340, 0, 66]", "[0, 100, 0, 66]");
    std::fs::write(&props, text).unwrap();
    std::fs::write(repo.join("ui").join("hud").join("Main.luau"), "return 2").unwrap();
    let (ok, out) = server.forge(&["sync", "diff"], &repo);
    assert!(ok, "{out}");
    assert!(out.contains("2 differences, disk -> Studio"), "{out}");
    assert!(
        out.contains("  ~ (top) Size: [0, 100, 0, 66] -> [0, 340, 0, 66]"),
        "{out}"
    );
    assert!(out.contains("  ~ Main script: +2 -1 lines"), "{out}");
}

#[test]
fn check_reports_each_value_the_copy_lost() {
    let server = Server::new("check");
    let repo = game_repo(&server);
    assert!(server.forge(&["status"], &repo).0);
    let requests = server.studio(42);
    let (ok, out) = server.forge(&["sync", "check", "StarterGui.Hud"], &repo);
    assert!(!ok, "a difference fails the check: {out}");
    assert!(
        out.contains("StarterGui.Hud: 2 instances, 1 differences"),
        "{out}"
    );
    assert!(
        out.contains("  ~ (top) Size: [0, 340, 0, 66] -> [0, 341, 0, 66]"),
        "{out}"
    );
    let check = requests.iter().find(|r| r["action"] == "check").unwrap();
    assert_eq!(check["copies"][0]["class"], "Frame");
    assert_eq!(check["copies"][0]["children"][0]["source"], SCRIPT);

    let (ok, out) = server.forge(&["sync", "check", "Nope"], &repo);
    assert!(!ok);
    assert!(out.contains("forge.toml has no tree Nope"), "{out}");
}
