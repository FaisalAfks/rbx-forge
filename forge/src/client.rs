//! A command's side of the link: connect to `forge serve` on this PC, starting it in the background when it isn't
//! running (decision 0015), say hello, and send requests. Every command that needs Studio goes through here.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as Frame;

use crate::protocol::{CliHello, Hello, Message, PROTOCOL, Repo};
use crate::settings::Settings;

/// How often a command checks whether the server it started takes connections yet.
const START_POLL: Duration = Duration::from_millis(100);

pub struct Link {
    socket: WebSocketStream<TcpStream>,
    /// True when this command started the server, so a plugin may not have found it yet.
    pub started: bool,
}

/// Connects to the server. With `start`, a server that isn't running is started first.
/// `settings_file` is the settings file in effect, which a server started here is given too.
pub async fn connect(
    settings: &Settings,
    settings_file: Option<&Path>,
    repo: Option<Repo>,
    start: bool,
) -> Result<Option<Link>> {
    let port = settings.server.port;
    let (stream, started) = match TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await {
        Ok(stream) => (stream, false),
        Err(_) if !start => return Ok(None),
        Err(_) => (start_server(settings, settings_file).await?, true),
    };
    let not_forge = || {
        anyhow!(
            "something other than forge serve is on 127.0.0.1:{port}: change `server.port` in the settings file \
             (`forge config`)"
        )
    };
    let (socket, _) = tokio_tungstenite::client_async(format!("ws://127.0.0.1:{port}"), stream)
        .await
        .map_err(|_| not_forge())?;
    let mut link = Link { socket, started };
    let hello = Message::Hello(Hello::Cli(CliHello {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        repo,
    }));
    match link.request(&hello).await.map_err(|_| not_forge())? {
        Message::Welcome { server, .. } if server == "forge" => Ok(Some(link)),
        Message::Error { message } => bail!("forge serve refused this command: {message}"),
        _ => Err(not_forge()),
    }
}

impl Link {
    /// Sends one message and waits for the answer.
    pub async fn request(&mut self, message: &Message) -> Result<Message> {
        self.socket
            .send(Frame::text(message.to_text()))
            .await
            .context("lost the connection to forge serve")?;
        loop {
            match self.socket.next().await {
                Some(Ok(Frame::Text(text))) => {
                    return Message::from_text(text.as_str())
                        .context("forge serve sent a message forge can't read");
                }
                Some(Ok(Frame::Close(_))) | None => bail!("forge serve closed the connection"),
                Some(Ok(_)) => {}
                Some(Err(err)) => return Err(err).context("lost the connection to forge serve"),
            }
        }
    }
}

/// Starts `forge serve` in the background and waits until it takes connections. It gets the settings file in effect
/// and writes its own log (`serve.log`, next to the settings file).
async fn start_server(settings: &Settings, settings_file: Option<&Path>) -> Result<TcpStream> {
    let port = settings.server.port;
    let timeout_ms = settings.server.start_timeout_ms;
    let log = log_file(settings_file)
        .context("this system has no settings folder for forge serve's log")?;
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("can't make {}", dir.display()))?;
    }
    let exe =
        std::env::current_exe().context("can't find the forge program to start forge serve")?;
    let mut args = vec![exe.display().to_string()];
    if let Some(file) = settings_file {
        args.extend(["--settings".to_string(), file.display().to_string()]);
    }
    args.extend([
        "serve".to_string(),
        "--log".to_string(),
        log.display().to_string(),
    ]);
    spawn_detached(&args)?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Ok(stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            bail!(
                "started forge serve, but it didn't take connections within {timeout_ms} ms \
                 (`server.start_timeout_ms`). Its log is {}; `forge serve` in a terminal shows why it stops",
                log.display()
            );
        }
        tokio::time::sleep(START_POLL).await;
    }
}

/// Starts a process that outlives this command and holds none of its handles.
///
/// On Windows a child inherits every inheritable handle of its parent, including the pipes a caller reads this
/// command's output from, so a caller waiting for the output to end (a script, `forge status | ...`, an MCP client)
/// would wait as long as the server runs. Rust's switch for that is nightly-only, and calling `CreateProcessW`
/// directly needs unsafe code, which the workspace forbids. So the server is created by Windows' own process service
/// (WMI's `Win32_Process.Create`), through PowerShell: it inherits nothing, has no window, and isn't closed with the
/// terminal. The command line goes through an environment variable, so no quoting passes through PowerShell.
#[cfg(windows)]
fn spawn_detached(args: &[String]) -> Result<()> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        $si = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0 }; \
        $r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ \
            CommandLine = $env:FORGE_SERVE_COMMAND; CurrentDirectory = $env:FORGE_SERVE_DIR; \
            ProcessStartupInformation = $si }; \
        exit $r.ReturnValue";
    let dir = std::env::temp_dir();
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("FORGE_SERVE_COMMAND", windows_command_line(args))
        .env("FORGE_SERVE_DIR", &dir)
        .stdin(Stdio::null())
        .output()
        .context("can't run PowerShell to start forge serve")?;
    if !out.status.success() {
        bail!(
            "Windows didn't start forge serve in the background (WMI said {:?}{}). Security software can block \
             this; run `forge serve` in a terminal window of its own instead",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Elsewhere a child keeps only the three standard handles, which are all null here; a process group of its own
/// keeps a Ctrl-C in the terminal from stopping it.
#[cfg(not(windows))]
fn spawn_detached(args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("can't start forge serve")?;
    Ok(())
}

/// Quotes arguments the way Windows programs (and Rust's own argument parser) read them back: in double quotes, with
/// backslashes doubled only where they come before a quote.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_command_line(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            let mut out = String::from("\"");
            let mut slashes = 0;
            for c in arg.chars() {
                match c {
                    '\\' => slashes += 1,
                    '"' => {
                        out.push_str(&"\\".repeat(slashes * 2 + 1));
                        out.push('"');
                        slashes = 0;
                    }
                    _ => {
                        out.push_str(&"\\".repeat(slashes));
                        out.push(c);
                        slashes = 0;
                    }
                }
            }
            // Backslashes at the end come right before the closing quote.
            out.push_str(&"\\".repeat(slashes * 2));
            out.push('"');
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `serve.log`, next to the settings file in effect.
pub fn log_file(settings_file: Option<&Path>) -> Option<PathBuf> {
    settings_file
        .and_then(Path::parent)
        .map(|dir| dir.join("serve.log"))
}

#[cfg(test)]
mod tests {
    use super::windows_command_line;

    fn line(args: &[&str]) -> String {
        windows_command_line(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn paths_with_spaces_are_quoted() {
        assert_eq!(
            line(&[r"C:\Users\Jane Doe\forge.exe", "serve"]),
            r#""C:\Users\Jane Doe\forge.exe" "serve""#
        );
    }

    #[test]
    fn backslashes_before_quotes_are_doubled() {
        assert_eq!(line(&[r"C:\dir\"]), r#""C:\dir\\""#);
        assert_eq!(line(&[r#"a\"b"#]), r#""a\\\"b""#);
    }
}
