//! `forge serve`: the one server per PC that holds forge's link (decisions 0012 and 0015). It listens on 127.0.0.1
//! only. Each Studio's plugin connects and stays connected; forge's commands connect, ask, and leave. The server
//! keeps who is connected and which repos commands ran in, so it can match places to repos. Messages are in
//! `protocol.rs` and docs/link.md.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as Frame;

use crate::protocol::{
    CliHello, Hello, Message, PROTOCOL, Repo, StatusReport, Studio, StudioHello,
};

/// How long a new connection has to say hello. Both of forge's sides send it at once, so only a stray connection
/// waits this long; it isn't a setting because nothing legitimate is ever near it.
const HELLO_WAIT: Duration = Duration::from_secs(10);

/// How long the server gives open connections to close once it's told to stop.
const STOP_WAIT: Duration = Duration::from_secs(2);

type Socket = WebSocketStream<TcpStream>;

/// What the server knows, shared by every connection.
#[derive(Default)]
struct State {
    next_id: u64,
    studios: BTreeMap<u64, (StudioHello, Instant)>,
    repos: BTreeSet<Repo>,
}

type Shared = Arc<Mutex<State>>;

/// Where the log goes when it isn't standard error: set once, when the server starts.
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Runs the server until a command sends `stop`. With `log`, its lines go to that file (overwritten, so it holds
/// this server's lines only) instead of standard error.
pub async fn run(port: u16, log_to: Option<&Path>) -> Result<()> {
    if let Some(path) = log_to {
        let file = std::fs::File::create(path)
            .with_context(|| format!("can't write {}", path.display()))?;
        let _ = LOG.set(Mutex::new(file));
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| {
            format!(
                "can't listen on 127.0.0.1:{port}. Either forge serve is already running (`forge status` says), \
                 or another program uses the port: change `server.port` in the settings file (`forge config`)"
            )
        })?;
    log(&format!(
        "forge serve {} on 127.0.0.1:{port}",
        env!("CARGO_PKG_VERSION")
    ));

    let state = Shared::default();
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    connections.spawn(serve_connection(stream, state.clone(), stop_tx.clone(), port));
                }
                Err(err) => log(&format!("a connection failed before it started: {err}")),
            },
            _ = stop_rx.changed() => break,
        }
    }

    log("stopping");
    // Each connection sees the stop and closes itself, so plugins get a clean close and start retrying.
    let _ = tokio::time::timeout(STOP_WAIT, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    Ok(())
}

async fn serve_connection(stream: TcpStream, state: Shared, stop: watch::Sender<bool>, port: u16) {
    let mut socket = match tokio_tungstenite::accept_async(stream).await {
        Ok(socket) => socket,
        Err(err) => return log(&format!("not a WebSocket connection: {err}")),
    };
    let hello = match tokio::time::timeout(HELLO_WAIT, next_message(&mut socket)).await {
        Ok(Some(Ok(Message::Hello(hello)))) => hello,
        Ok(Some(Ok(other))) => {
            return refuse(&mut socket, &format!("expected hello first, got {other:?}")).await;
        }
        Ok(Some(Err(err))) => return refuse(&mut socket, &err).await,
        Ok(None) => return,
        Err(_) => return refuse(&mut socket, "no hello in time").await,
    };
    let welcome = Message::Welcome {
        server: "forge".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: PROTOCOL,
    };
    match hello {
        Hello::Studio(h) if h.protocol != PROTOCOL => {
            let msg = format!(
                "the plugin speaks link protocol {} and forge serve speaks {PROTOCOL}: run `forge plugin install` \
                 and restart Studio",
                h.protocol
            );
            refuse(&mut socket, &msg).await
        }
        Hello::Cli(h) if h.protocol != PROTOCOL => {
            let msg = format!(
                "this forge ({}) speaks link protocol {} and the running forge serve ({}) speaks {PROTOCOL}: run \
                 `forge stop` with the forge that started it, then try again",
                h.version,
                h.protocol,
                env!("CARGO_PKG_VERSION")
            );
            refuse(&mut socket, &msg).await
        }
        Hello::Studio(h) => {
            if send(&mut socket, &welcome).await.is_ok() {
                serve_studio(socket, h, state, stop.subscribe()).await;
            }
        }
        Hello::Cli(h) => {
            if send(&mut socket, &welcome).await.is_ok() {
                serve_cli(socket, h, state, stop, port).await;
            }
        }
    }
}

/// Holds a plugin's connection until it closes or the server stops. Later steps add the messages Studio sends.
async fn serve_studio(
    mut socket: Socket,
    hello: StudioHello,
    state: Shared,
    mut stop: watch::Receiver<bool>,
) {
    let id = {
        let mut s = state.lock().expect("state lock");
        s.next_id += 1;
        let id = s.next_id;
        s.studios.insert(id, (hello.clone(), Instant::now()));
        id
    };
    let who = format!(
        "Studio on place {} ({:?}, session {})",
        hello.place, hello.name, hello.session
    );
    log(&format!("{who} connected"));
    loop {
        tokio::select! {
            message = next_message(&mut socket) => match message {
                None => break,
                Some(Ok(other)) => {
                    let reply = Message::Error { message: format!("forge serve doesn't take {other:?} from Studio") };
                    if send(&mut socket, &reply).await.is_err() {
                        break;
                    }
                }
                Some(Err(err)) => log(&format!("{who}: {err}")),
            },
            _ = stop.changed() => {
                let _ = socket.close(None).await;
                break;
            }
        }
    }
    state.lock().expect("state lock").studios.remove(&id);
    log(&format!("{who} disconnected"));
}

/// Answers a command's requests until it leaves.
async fn serve_cli(
    mut socket: Socket,
    hello: CliHello,
    state: Shared,
    stop: watch::Sender<bool>,
    port: u16,
) {
    if let Some(repo) = hello.repo {
        state.lock().expect("state lock").repos.insert(repo);
    }
    while let Some(message) = next_message(&mut socket).await {
        let reply = match message {
            Ok(Message::Status) => Message::StatusReport(report(&state, port)),
            Ok(Message::Stop) => {
                let _ = send(&mut socket, &Message::Stopping).await;
                let _ = socket.close(None).await;
                let _ = stop.send(true);
                return;
            }
            Ok(other) => Message::Error {
                message: format!("forge serve doesn't take {other:?} from a command"),
            },
            Err(err) => Message::Error { message: err },
        };
        if send(&mut socket, &reply).await.is_err() {
            return;
        }
    }
}

fn report(state: &Shared, port: u16) -> StatusReport {
    let s = state.lock().expect("state lock");
    StatusReport {
        version: env!("CARGO_PKG_VERSION").into(),
        port,
        studios: s
            .studios
            .values()
            .map(|(h, since)| Studio {
                session: h.session.clone(),
                place: h.place,
                game: h.game,
                name: h.name.clone(),
                plugin: h.plugin.clone(),
                connected_secs: since.elapsed().as_secs(),
            })
            .collect(),
        repos: s.repos.iter().cloned().collect(),
    }
}

/// The next message, `Some(Err)` for a frame that isn't one, or `None` once the connection is closed. Pings are
/// answered by the WebSocket library while reading.
async fn next_message(socket: &mut Socket) -> Option<Result<Message, String>> {
    loop {
        match socket.next().await? {
            Ok(Frame::Text(text)) => {
                return Some(
                    Message::from_text(text.as_str())
                        .map_err(|e| format!("a message forge can't read: {e}")),
                );
            }
            Ok(Frame::Close(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

async fn send(socket: &mut Socket, message: &Message) -> Result<(), ()> {
    socket
        .send(Frame::text(message.to_text()))
        .await
        .map_err(|_| ())
}

async fn refuse(socket: &mut Socket, message: &str) {
    log(&format!("refused a connection: {message}"));
    let _ = send(
        socket,
        &Message::Error {
            message: message.to_string(),
        },
    )
    .await;
    let _ = socket.close(None).await;
}

/// One line of the server's log.
fn log(line: &str) {
    match LOG.get() {
        Some(file) => {
            let mut file = file.lock().expect("log lock");
            let _ = writeln!(file, "{line}");
        }
        None => eprintln!("{line}"),
    }
}
