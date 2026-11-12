//! What every forge tool uses (decision 0019): the link to Studio (`server`, `client`, `protocol`), forge's settings,
//! a game's `forge.toml`, and the `Tool` interface each tool fills in. Core knows no tool by name; the binary hands it
//! the list.

/// Prints a line to standard output. When nothing reads it any more (`forge sync diff | head`), the command ends there
/// instead of failing, as command-line tools do.
#[macro_export]
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if writeln!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

pub mod client;
pub mod game;
pub mod protocol;
pub mod server;
pub mod session;
pub mod settings;
pub mod tool;

use anyhow::{Context, Result};

pub use tool::{Cx, Kind, McpTool, Param, Tool};

/// Runs a command's async work. The link is async; one thread is plenty for a handful of connections.
pub fn block_on<F: std::future::Future<Output = Result<()>>>(future: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("can't start forge's async runtime")?
        .block_on(future)
}
