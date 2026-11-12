//! `forge serve`: the one server per PC that holds forge's link (decisions 0012 and 0015). It listens on 127.0.0.1
//! only. Each Studio's plugin connects and stays connected; forge's commands connect, ask, and leave. The server
//! keeps who is connected and which repos commands ran in, so it can match places to repos, and passes a command's
//! request to the Studio on its place and the answer back. It doesn't know what tools ask: the plugin does. Messages
//! are in `protocol.rs` and docs/link.md.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value as Json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as Frame};

use crate::protocol::{
    CliHello, Hello, Message, PROTOCOL, Refused, Repo, StatusReport, Studio, StudioHello,
};

/// How long a new connection has to say hello. Both of forge's sides send it at once, so only a stray connection
/// waits this long; it isn't a setting because nothing legitimate is ever near it.
const HELLO_WAIT: Duration = Duration::from_secs(10);

/// How long the server gives open connections to close once it's told to stop.
const STOP_WAIT: Duration = Duration::from_secs(2);

type Socket = WebSocketStream<TcpStream>;

/// The server's settings, from forge's settings file.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub port: u16,
    /// How long a command's request may wait for Studio's answer.
    pub request_timeout: Duration,
    /// The largest message the server takes or sends; a bigger one closes that connection.
    pub max_message_bytes: usize,
}

/// WebSocket limits for one side of the link. Studio sends each message as one frame, so both limits are the
/// message size.
pub fn websocket_config(max_bytes: usize) -> WebSocketConfig {
    let mut c = WebSocketConfig::default();
    c.max_message_size = Some(max_bytes);
    c.max_frame_size = Some(max_bytes);
    c
}

/// A Studio linked to the server.
struct Linked {
    hello: StudioHello,
    since: Instant,
    /// Requests for this Studio, by id; its connection sends them on.
    requests: mpsc::UnboundedSender<(u64, Json)>,
}

/// Studio's answer to a request: its reply, or why it failed.
type Answer = Result<Json, String>;

/// What the server knows, shared by every connection.
#[derive(Default)]
struct State {
    next_id: u64,
    studios: BTreeMap<u64, Linked>,
    repos: BTreeSet<Repo>,
    /// Requests sent to a Studio and not answered yet: the Studio's id, and where the answer goes.
    pending: HashMap<u64, (u64, oneshot::Sender<Answer>)>,
    /// Places whose plugin was turned away (an old protocol), and why, until a plugin on that place links.
    refused: BTreeMap<u64, String>,
}

type Shared = Arc<Mutex<State>>;

/// Where the log goes when it isn't standard error: set once, when the server starts.
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Runs the server until a command sends `stop`. With `log`, its lines go to that file instead of standard error.
/// The file is overwritten once the port is taken, so it holds the running server's lines only; a server that can't
/// take the port adds its reason at the end instead, leaving the running server's lines in place.
pub async fn run(config: Config, log_to: Option<&Path>) -> Result<()> {
    let port = config.port;
    let bound = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await;
    if let Some(path) = log_to {
        let mut options = std::fs::OpenOptions::new();
        match bound {
            Ok(_) => options.write(true).create(true).truncate(true),
            Err(_) => options.append(true).create(true),
        };
        let file = options
            .open(path)
            .with_context(|| format!("can't write {}", path.display()))?;
        let _ = LOG.set(Mutex::new(file));
    }
    let listener = match bound {
        Ok(listener) => listener,
        Err(err) => {
            let message = format!(
                "can't listen on 127.0.0.1:{port} ({err}). Either forge serve is already running (`forge status` \
                 says), or another program uses the port: change `server.port` in the settings file (`forge config`)"
            );
            // A server started in the background has no terminal, so the reason goes in its log too.
            if LOG.get().is_some() {
                log(&message);
            }
            bail!(message);
        }
    };
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
                    connections.spawn(serve_connection(stream, state.clone(), stop_tx.clone(), config));
                }
                Err(err) => log(&format!("a connection failed before it started: {err}")),
            },
            _ = stop_rx.changed() => break,
        }
    }

    log("stopping");
    // The port is free from here, so a new server can start while these connections close.
    drop(listener);
    // Each connection sees the stop and closes itself, so plugins get a clean close and start retrying.
    let _ = tokio::time::timeout(STOP_WAIT, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    Ok(())
}

async fn serve_connection(
    stream: TcpStream,
    state: Shared,
    stop: watch::Sender<bool>,
    config: Config,
) {
    let limits = websocket_config(config.max_message_bytes);
    let mut socket = match tokio_tungstenite::accept_async_with_config(stream, Some(limits)).await {
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
            // Kept so `forge status` and a command asking for this place can say why it isn't linked.
            state
                .lock()
                .expect("state lock")
                .refused
                .insert(h.place, msg.clone());
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
                serve_cli(socket, h, state, stop, config).await;
            }
        }
    }
}

/// Holds a plugin's connection until it closes or the server stops: sends it the requests meant for it, and passes
/// its answers back.
async fn serve_studio(
    mut socket: Socket,
    hello: StudioHello,
    state: Shared,
    mut stop: watch::Receiver<bool>,
) {
    let (tx, mut requests) = mpsc::unbounded_channel();
    let id = {
        let mut s = state.lock().expect("state lock");
        s.next_id += 1;
        let id = s.next_id;
        let linked = Linked {
            hello: hello.clone(),
            since: Instant::now(),
            requests: tx,
        };
        s.studios.insert(id, linked);
        s.refused.remove(&hello.place);
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
                Some(Ok(Message::Reply { id: request, reply })) => answer(&state, request, Ok(reply)),
                Some(Ok(Message::Failed { id: request, message })) => answer(&state, request, Err(message)),
                Some(Ok(other)) => {
                    let reply = Message::Error { message: format!("forge serve doesn't take {other:?} from Studio") };
                    if send(&mut socket, &reply).await.is_err() {
                        break;
                    }
                }
                Some(Err(err)) => log(&format!("{who}: {err}")),
            },
            Some((request, body)) = requests.recv() => {
                if send(&mut socket, &Message::Request { id: request, request: body }).await.is_err() {
                    break;
                }
            }
            _ = stop.changed() => {
                let _ = socket.close(None).await;
                break;
            }
        }
    }
    let mut s = state.lock().expect("state lock");
    s.studios.remove(&id);
    // Requests this Studio will never answer: dropping their senders tells each waiting command.
    s.pending.retain(|_, (studio, _)| *studio != id);
    drop(s);
    log(&format!("{who} disconnected"));
}

fn answer(state: &Shared, request: u64, answer: Answer) {
    let waiting = state.lock().expect("state lock").pending.remove(&request);
    if let Some((_, tx)) = waiting {
        let _ = tx.send(answer);
    }
}

/// Answers a command's requests until it leaves.
async fn serve_cli(
    mut socket: Socket,
    hello: CliHello,
    state: Shared,
    stop: watch::Sender<bool>,
    config: Config,
) {
    if let Some(repo) = hello.repo {
        state.lock().expect("state lock").repos.insert(repo);
    }
    while let Some(message) = next_message(&mut socket).await {
        let reply = match message {
            Ok(Message::Status) => Message::StatusReport(report(&state, config.port)),
            Ok(Message::Stop) => {
                let _ = send(&mut socket, &Message::Stopping).await;
                let _ = socket.close(None).await;
                let _ = stop.send(true);
                return;
            }
            Ok(Message::ToStudio {
                place,
                request,
                timeout_ms,
            }) => {
                let mut config = config;
                if let Some(ms) = timeout_ms {
                    config.request_timeout = Duration::from_millis(ms);
                }
                match ask_studio(&state, place, request, config).await {
                    Ok(reply) => Message::FromStudio { reply },
                    Err(message) => Message::Error { message },
                }
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

/// Sends a request to the one Studio on `place` and waits for its answer.
async fn ask_studio(state: &Shared, place: u64, request: Json, config: Config) -> Answer {
    let rx = {
        let mut s = state.lock().expect("state lock");
        let on_place: Vec<(u64, &Linked)> = s
            .studios
            .iter()
            .filter(|(_, l)| l.hello.place == place)
            .map(|(id, l)| (*id, l))
            .collect();
        let (studio, sender) = match on_place.as_slice() {
            [] => {
                return Err(match s.refused.get(&place) {
                    Some(why) => format!("the Studio with place {place} couldn't link: {why}"),
                    None => format!(
                        "no Studio with place {place} is linked to forge serve: open the place in Studio, with \
                         forge's plugin installed (`forge status` lists what's linked)"
                    ),
                });
            }
            [(id, linked)] => (*id, linked.requests.clone()),
            many => {
                let sessions: Vec<&str> =
                    many.iter().map(|(_, l)| l.hello.session.as_str()).collect();
                return Err(format!(
                    "{} Studios have place {place} open (sessions {}): close all but one",
                    many.len(),
                    sessions.join(", ")
                ));
            }
        };
        s.next_id += 1;
        let request_id = s.next_id;
        let (tx, rx) = oneshot::channel();
        s.pending.insert(request_id, (studio, tx));
        if sender.send((request_id, request)).is_err() {
            s.pending.remove(&request_id);
            return Err("the Studio on that place just closed its link".into());
        }
        rx
    };
    match tokio::time::timeout(config.request_timeout, rx).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(_)) => Err(format!(
            "the Studio on place {place} closed its link before answering (a message over \
             `server.max_message_mb` closes it too; the server's log says)"
        )),
        Err(_) => Err(format!(
            "Studio didn't answer within {} ms (`server.request_timeout_ms`)",
            config.request_timeout.as_millis()
        )),
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
            .map(|l| Studio {
                session: l.hello.session.clone(),
                place: l.hello.place,
                game: l.hello.game,
                name: l.hello.name.clone(),
                plugin: l.hello.plugin.clone(),
                connected_secs: l.since.elapsed().as_secs(),
            })
            .collect(),
        repos: s.repos.iter().cloned().collect(),
        refused: s
            .refused
            .iter()
            .map(|(place, message)| Refused {
                place: *place,
                message: message.clone(),
            })
            .collect(),
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
            Ok(Frame::Close(_)) => return None,
            Err(err) => {
                // A closed connection is normal; anything else (a message over the size limit) is worth a line.
                if !matches!(err, WsError::ConnectionClosed | WsError::AlreadyClosed) {
                    log(&format!("a connection ended: {err}"));
                }
                return None;
            }
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
