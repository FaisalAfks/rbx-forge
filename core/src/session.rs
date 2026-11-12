//! A tool's session with Studio: the game around the current folder and a link to the Studio that has its place open,
//! starting `forge serve` when it isn't running.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::client::{self, Link};
use crate::game::Game;
use crate::protocol::Message;
use crate::settings::Settings;
use crate::tool::Cx;

/// Links to the server for `game`, and gives a server this command just started time to find its Studio.
pub async fn link(cx: &Cx, settings: &Settings, game: &Game) -> Result<Link> {
    let mut link = client::connect(settings, cx.settings_file(), cx.this_repo()?, true)
        .await?
        .expect("connect starts the server when asked to");
    if link.started {
        wait_for_studio(&mut link, settings, game.place).await?;
    }
    Ok(link)
}

/// A server this command just started has no Studio yet: the plugin finds it on its next try.
async fn wait_for_studio(link: &mut Link, settings: &Settings, place: u64) -> Result<()> {
    let wait = Duration::from_millis(settings.plugin.retry_ms + settings.plugin.open_timeout_ms);
    let deadline = Instant::now() + wait;
    loop {
        let linked = match link.request(&Message::Status).await? {
            Message::StatusReport(r) => r.studios.iter().any(|s| s.place == place),
            other => bail!("forge serve answered status with {other:?}"),
        };
        if linked || Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
