//! `forge mcp` as an MCP client sees it, without Studio: the handshake, the tools, and a tool whose command fails
//! coming back as an error with the command's own message.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

#[test]
fn an_mcp_client_lists_the_tools_and_gets_errors_back() {
    let dir = std::env::temp_dir().join(format!("forge mcp {}", std::process::id()));
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
    std::fs::write(
        dir.join("forge.toml"),
        "place = 42\n\n[[sync]]\nstudio = \"StarterGui.Hud\"\ndisk = \"ui/hud\"\n",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("mcp")
        .current_dir(&dir)
        .env("FORGE_SETTINGS", &settings)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("forge mcp runs");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str(&line) {
                let _ = tx.send(v);
            }
        }
    });
    let mut send = |v: Value| writeln!(stdin, "{v}").unwrap();
    let reply = |id: u64| loop {
        let v = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("forge mcp answers");
        if v["id"] == id {
            return v;
        }
    };

    send(
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }),
    );
    let init = reply(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "forge");
    send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let mut names: Vec<String> = reply(2)["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["check", "clashes", "diff", "pick", "pull", "push", "status"]
    );

    // No Studio has place 42, so diff fails, and the MCP says so with the command's own words.
    send(
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "diff", "arguments": {} } }),
    );
    let diff = reply(3);
    assert_eq!(diff["result"]["isError"], true, "{diff}");
    let text = diff["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("no Studio with place 42"), "{text}");

    drop(send);
    let _ = child.kill();
    let _ = child.wait();
    let _ = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["stop"])
        .current_dir(&dir)
        .env("FORGE_SETTINGS", &settings)
        .output();
}
