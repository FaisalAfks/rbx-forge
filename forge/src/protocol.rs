//! The messages on forge's link (docs/link.md): JSON text frames between `forge serve`, its Studio plugin and forge's
//! own commands, each tagged by `op`. Every connection starts with a `hello` and gets a `welcome` or an `error`.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// Bumped when a message changes shape, so an old plugin or command gets a clear error instead of a wrong reading.
/// 2: requests passed to Studio (`to_studio`, `request`, `reply`, `failed`).
/// 3: events from Studio for watch (`notify`, `subscribe`, `event`).
pub const PROTOCOL: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Message {
    /// The first message on every connection.
    Hello(Hello),
    /// The server's answer to a hello it accepts.
    Welcome {
        server: String,
        version: String,
        protocol: u32,
    },
    /// Something the other side can't do. A bad hello is answered with this, then the connection is closed.
    Error {
        message: String,
    },
    /// A command asks what's connected.
    Status,
    StatusReport(StatusReport),
    /// A command asks the server to stop. The server answers `stopping`, closes every connection and exits.
    Stop,
    Stopping,
    /// A command asks the Studio on `place` something. The server passes `request` on and answers `from_studio`, or
    /// `error` when no single Studio has that place open or it doesn't answer in time.
    ToStudio {
        place: u64,
        request: Json,
        /// How long this request may wait, when it isn't `server.request_timeout_ms` (a push waits for a click).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    FromStudio {
        reply: Json,
    },
    /// The server passes a command's request to a plugin.
    Request {
        id: u64,
        request: Json,
    },
    /// The plugin's answer to request `id`.
    Reply {
        id: u64,
        reply: Json,
    },
    /// The plugin couldn't do request `id`.
    Failed {
        id: u64,
        message: String,
    },
    /// A command asks for the events of the Studio on `place` (watch). The server answers `subscribed`, then sends
    /// each `event` on this connection until it closes.
    Subscribe {
        place: u64,
    },
    Subscribed,
    /// The plugin tells the server something happened (a tree changed); the server passes it on as `event`.
    Notify {
        event: Json,
    },
    Event {
        place: u64,
        event: Json,
    },
}

/// Who is connecting, and what the server needs to know about them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Hello {
    Studio(StudioHello),
    Cli(CliHello),
}

/// forge's plugin, from one Studio's edit session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudioHello {
    pub protocol: u32,
    /// The forge version that packed the plugin.
    pub plugin: String,
    /// Made up by the plugin when it starts, so two Studios on the same place are told apart.
    pub session: String,
    /// `game.PlaceId`; 0 for a place that was never published.
    pub place: u64,
    /// `game.GameId`, the universe.
    pub game: u64,
    pub name: String,
}

/// One of forge's commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliHello {
    pub protocol: u32,
    pub version: String,
    /// The game repo the command runs in, if it runs in one. The server remembers it, to match places to repos.
    pub repo: Option<Repo>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Repo {
    /// The folder that holds the repo's `forge.toml`.
    pub root: String,
    pub place: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusReport {
    pub version: String,
    pub port: u16,
    pub studios: Vec<Studio>,
    /// Every repo a command has run in since the server started.
    pub repos: Vec<Repo>,
    /// Places whose plugin the server turned away, and why.
    #[serde(default)]
    pub refused: Vec<Refused>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refused {
    pub place: u64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Studio {
    pub session: String,
    pub place: u64,
    pub game: u64,
    pub name: String,
    pub plugin: String,
    pub connected_secs: u64,
}

impl Message {
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).expect("a message always turns into JSON")
    }

    pub fn from_text(text: &str) -> Result<Message, serde_json::Error> {
        serde_json::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_studio_hello_reads_as_the_plugin_writes_it() {
        // The plugin's JSONEncode gives keys in any order and whole numbers without a decimal point.
        let text = r#"{"name":"Place1","op":"hello","session":"A1","place":123456789012345,"role":"studio","game":98765432109,"protocol":1,"plugin":"0.0.0"}"#;
        let Message::Hello(Hello::Studio(h)) = Message::from_text(text).unwrap() else {
            panic!("not a studio hello");
        };
        assert_eq!(h.place, 123456789012345);
        assert_eq!(h.game, 98765432109);
        assert_eq!(h.name, "Place1");
    }

    #[test]
    fn every_message_reads_back_as_itself() {
        let all = [
            Message::Hello(Hello::Cli(CliHello {
                protocol: PROTOCOL,
                version: "1.2.3".into(),
                repo: Some(Repo {
                    root: "C:\\games\\x".into(),
                    place: 7,
                }),
            })),
            Message::Welcome {
                server: "forge".into(),
                version: "1.2.3".into(),
                protocol: PROTOCOL,
            },
            Message::Error {
                message: "no".into(),
            },
            Message::Status,
            Message::StatusReport(StatusReport {
                version: "1.2.3".into(),
                port: 34900,
                studios: vec![Studio {
                    session: "s".into(),
                    place: 1,
                    game: 2,
                    name: "n".into(),
                    plugin: "1.2.3".into(),
                    connected_secs: 5,
                }],
                repos: vec![],
                refused: vec![Refused {
                    place: 5,
                    message: "old".into(),
                }],
            }),
            Message::Stop,
            Message::Stopping,
            Message::ToStudio {
                place: 3,
                request: serde_json::json!({ "tool": "sync" }),
                timeout_ms: Some(5),
            },
            Message::FromStudio {
                reply: serde_json::json!([1, 2]),
            },
            Message::Request {
                id: 4,
                request: serde_json::json!({}),
            },
            Message::Reply {
                id: 4,
                reply: serde_json::json!(null),
            },
            Message::Failed {
                id: 4,
                message: "no".into(),
            },
            Message::Subscribe { place: 5 },
            Message::Subscribed,
            Message::Notify {
                event: serde_json::json!({ "tree": 0 }),
            },
            Message::Event {
                place: 5,
                event: serde_json::json!({ "tree": 0 }),
            },
        ];
        for m in all {
            assert_eq!(Message::from_text(&m.to_text()).unwrap(), m);
        }
    }

    #[test]
    fn ops_are_snake_case_on_the_wire() {
        assert_eq!(Message::Stopping.to_text(), r#"{"op":"stopping"}"#);
        assert!(Message::Status.to_text().contains(r#""op":"status""#));
    }
}
