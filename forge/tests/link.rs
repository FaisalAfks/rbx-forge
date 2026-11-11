//! The link end to end, without Studio: the real `forge` binary as the server and commands, and a small WebSocket
//! client standing in for the plugin. The plugin's own side (retrying in Studio) is checked in Studio.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tungstenite::{Message, WebSocket};

/// A server of its own for one test: its own port and settings file, stopped when the test ends, pass or fail.
struct Server {
    dir: PathBuf,
    settings: PathBuf,
    port: u16,
}

impl Server {
    fn new(name: &str) -> Server {
        // A space in the path checks that the server's command line is quoted.
        let dir = std::env::temp_dir().join(format!("forge link {}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A port the system just handed out is free; tests run in parallel, so each needs its own.
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

    /// Runs forge with its output in pipes, as scripts and MCP clients do. A server it starts must not hold those
    /// pipes open (on Windows it inherits them unless started apart), or the output never ends: that fails here.
    fn forge(&self, args: &[&str], cwd: &Path) -> Output {
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
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(part) => text.push_str(&part),
                Err(_) => panic!(
                    "forge {args:?}: its output never ended, so a background server held the pipe"
                ),
            }
        }
        Output {
            status: child.wait().unwrap(),
            text,
        }
    }

    fn studio(&self, place: u64, protocol: u32) -> (WebSocket<TcpStream>, String) {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        // A server that never answers or closes fails the test instead of hanging it.
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let (mut ws, _) =
            tungstenite::client(format!("ws://127.0.0.1:{}", self.port), stream).unwrap();
        let hello = format!(
            r#"{{"op":"hello","role":"studio","protocol":{protocol},"plugin":"{}","session":"S{place}","place":{place},"game":9,"name":"Place1"}}"#,
            env!("CARGO_PKG_VERSION")
        );
        ws.send(Message::text(hello)).unwrap();
        let reply = ws.read().unwrap().into_text().unwrap().to_string();
        (ws, reply)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.forge(&["stop"], &self.dir.clone());
    }
}

struct Output {
    status: ExitStatus,
    text: String,
}

fn text(out: &Output) -> String {
    out.text.clone()
}

fn game_repo(server: &Server, place: u64) -> PathBuf {
    let repo = server.dir.join("game");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("forge.toml"), format!("place = {place}\n")).unwrap();
    repo
}

#[test]
fn status_starts_the_server_and_lists_a_studio_with_its_repo() {
    let server = Server::new("status");
    let repo = game_repo(&server, 42);

    let first = server.forge(&["status"], &repo);
    let first_text = text(&first);
    assert!(first.status.success(), "{first_text}");
    assert!(first_text.contains("started forge serve"), "{first_text}");
    assert!(
        first_text.contains("no Studio connected: is Studio open, with forge's plugin installed? (`forge plugin install`, then restart Studio)"),
        "{first_text}"
    );
    assert!(
        first_text.contains("repo with no Studio open"),
        "{first_text}"
    );

    let (_studio, welcome) = server.studio(42, 1);
    assert!(welcome.contains(r#""op":"welcome""#), "{welcome}");

    let second = text(&server.forge(&["status"], &repo));
    assert!(!second.contains("started forge serve"), "{second}");
    assert!(
        second.contains("studio: place 42 (game 9) \"Place1\""),
        "{second}"
    );
    assert!(
        second.contains(&format!("repo: {}", repo.display())),
        "{second}"
    );
}

#[test]
fn a_studio_links_up_again_after_the_server_restarts() {
    let server = Server::new("restart");
    let repo = game_repo(&server, 7);
    assert!(server.forge(&["status"], &repo).status.success());
    let (mut studio, _) = server.studio(7, 1);

    let stopped = text(&server.forge(&["stop"], &repo));
    assert!(stopped.contains("stopped forge serve"), "{stopped}");
    // The plugin sees its connection close, which is when it starts trying again.
    loop {
        match studio.read() {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    assert!(text(&server.forge(&["stop"], &repo)).contains("isn't running"));

    let restarted = text(&server.forge(&["status"], &repo));
    assert!(restarted.contains("started forge serve"), "{restarted}");
    let (_studio, welcome) = server.studio(7, 1);
    assert!(welcome.contains("welcome"), "{welcome}");
    assert!(text(&server.forge(&["status"], &repo)).contains("studio: place 7"));
}

#[test]
fn an_old_plugin_is_told_to_reinstall() {
    let server = Server::new("protocol");
    assert!(server.forge(&["status"], &server.dir).status.success());
    let (_studio, reply) = server.studio(1, 999);
    assert!(reply.contains(r#""op":"error""#), "{reply}");
    assert!(reply.contains("forge plugin install"), "{reply}");
}

#[test]
fn a_second_server_on_the_same_port_says_why_it_cant_start() {
    let server = Server::new("twice");
    assert!(server.forge(&["status"], &server.dir).status.success());
    let out = server.forge(&["serve"], &server.dir);
    assert!(!out.status.success());
    assert!(text(&out).contains("already running"), "{}", text(&out));
}

#[test]
fn plugin_install_writes_the_file_where_asked() {
    let server = Server::new("install");
    let out_file = server.dir.join("forge.rbxmx");
    let out = server.forge(
        &["plugin", "install", "--out", out_file.to_str().unwrap()],
        &server.dir,
    );
    assert!(out.status.success(), "{}", text(&out));
    let xml = std::fs::read_to_string(&out_file).unwrap();
    assert!(xml.contains("<roblox"), "not a model file");
    assert!(xml.contains("ModuleScript"));
}
