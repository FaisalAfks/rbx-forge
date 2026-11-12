# forge's link: the server, the plugin and the messages

How forge's commands reach Studio (decisions 0012 and 0015). Code: `forge/src/server.rs`, `client.rs` and
`protocol.rs`, and `plugin/link.luau`.

## The parts

- **`forge serve`**: one per PC, listening on `127.0.0.1` only, on `server.port` (34900 by default). It holds a
  connection to each open Studio and answers forge's commands.
- **forge's plugin** (`plugin/`), in each Studio: a WebSocket client that connects to the server. It runs in the edit
  session only, not in a playtest's client or server.
- **forge's commands** (`forge status`, and later `sync pull`, `push`, `forge mcp`): each one connects, asks, and
  leaves.

Nobody has to start the server. A command that needs it and finds nothing on the port starts it in the background,
waits up to `server.start_timeout_ms` for it to take connections, and goes on. `forge serve` in a terminal runs it in
that window instead, with its log printed there. `forge stop` stops it.

### Starting it in the background

The started server writes its log to `serve.log`, next to the settings file (`forge config` names it). The log is
overwritten at each start.

On Windows the server is created by Windows' process service (WMI's `Win32_Process.Create`, through PowerShell), not
as a child of the command. A child would inherit the command's output pipes, so anything reading the command's output
(a script, `forge status | ...`, an MCP client) would wait for as long as the server runs. Two other ways were
weighed:
- Rust's switch to stop inheriting handles is nightly-only.
- Calling `CreateProcessW` directly needs unsafe code, which the workspace forbids.

WMI starts it with no window, and it keeps running when the terminal closes. The cost is about a second at the first
start, and some security software blocks WMI process creation. forge then says so; `forge serve` in a window of its
own works the same.

The new server doesn't see the command's environment, so the command passes the settings file it used with
`--settings`.

## The plugin

forge's button in Studio's Plugins tab opens its window (#59): a home page with the link's state (and Connect or
Disconnect), a push waiting for Apply, and the last few things commands asked for. Settings and a push's full plan
open as their own views. The settings (`plugin/settings.luau`) are stored with `plugin:SetSetting`, so they last
across restarts and only change in Studio:
- **Apply pushes**: ask every time, ask for large pushes, or never ask (`sync/push.md`, decision 0018);
- **Connect automatically**: start the link when the place opens (on by default); off, the window's Connect starts it.

`forge plugin install` packs the plugin into `forge.rbxmx` and writes it to Studio's Plugins folder
(`%LOCALAPPDATA%\Roblox\Plugins` on Windows, `~/Documents/Roblox/Plugins` on macOS), or to `--out`. Studio loads
plugin files only when it starts, so Studio has to restart after an install.

The plugin's Luau is built into the forge binary, so the install works wherever forge is. How files become instances:

| On disk | In the plugin |
|---|---|
| `plugin/main.luau` | the Script `forge` |
| `plugin/<name>.luau` | a ModuleScript `<name>` under it |
| `plugin/<folder>/` | a Folder, packed the same way |
| `tools/<tool>/studio/` | the Folder `tools/<tool>` under it, packed the same way |

The settings the plugin needs are attributes on its Script, written at install from forge's settings: `port`,
`retryMs` (`plugin.retry_ms`), `openTimeoutMs` (`plugin.open_timeout_ms`), `version` and `protocol`. A changed
setting needs a reinstall.

`forge plugin sourcemap` prints the same layout as a Rojo-style sourcemap, so luau-lsp can follow `require(script.x)`
(CI uses it).

### How the plugin connects

The plugin tries to connect over and over in the background:
1. It opens a WebSocket to `ws://127.0.0.1:<port>` and says hello.
2. It holds the connection until it closes, or until `openTimeoutMs` passes without it opening.
3. It closes the client, waits `retryMs`, and tries again.

It never prompts and never blocks Studio, and a failed try prints nothing. Studio reports a refused connection as
`Error` ("ConnectFail") then `Closed` about 2 s later, and logs nothing of its own. Closing each client frees it:
Studio allows four at a time, and seven tries in a row each got a fresh one.

The plugin prints one line when it links up and one when the link is lost. An error from the server (an old plugin,
for example) is shown once, not on every try.

## Messages

JSON text frames, each an object with an `op`. Every connection starts with `hello`. The server answers `welcome`, or
`error` and closes the connection. `protocol` is 3, and is raised when a message changes shape. A plugin or command
with another protocol gets an error that says what to run. (2 added requests to Studio; a plugin installed before it
is told to reinstall.)

| op | From | Fields | Answer |
|---|---|---|---|
| `hello` | plugin | `role = "studio"`, `protocol`, `plugin` (the forge version that packed it), `session` (made up at plugin start), `place` (`game.PlaceId`, 0 if never published), `game` (`game.GameId`), `name` (`game.Name`) | `welcome` |
| `hello` | command | `role = "cli"`, `protocol`, `version`, `repo`: `{ root, place }` of the `forge.toml` it runs under, or null | `welcome` |
| `welcome` | server | `server = "forge"`, `version`, `protocol` | |
| `error` | server | `message` | |
| `status` | command | | `status_report`: `version`, `port`, `studios` (each: `session`, `place`, `game`, `name`, `plugin`, `connected_secs`), `repos` (each: `root`, `place`), `refused` (each: `place`, `message`: a plugin turned away, for an old protocol, until one on that place links) |
| `stop` | command | | `stopping`, then the server closes every connection and exits |
| `subscribe` | command | `place` | `subscribed`, then an `event` (`place`, `event`) for each `notify` from that place's plugin, until the command leaves |
| `notify` | plugin | `event` | (passed on to subscribers) |
| `to_studio` | command | `place`, `request` (any JSON, for the plugin), `timeout_ms` (optional: instead of `server.request_timeout_ms`, for a push waiting on a click) | `from_studio` with `reply`, or `error` |
| `request` | server | `id`, `request` | `reply` with `id` and `reply`, or `failed` with `id` and `message` |

A message the server doesn't take is answered with `error`. Later steps add the messages for watch.

### Requests to Studio

The server passes a command's `request` on to the one Studio that has `place` open, and its answer back. It doesn't
look inside: the plugin hands each request to the tool its `tool` field names (`sync` for now), and that tool's
`action` says what to do. The command gets an `error` when:
- no Studio with that place is linked;
- more than one is (close all but one);
- Studio doesn't answer within `server.request_timeout_ms` (two minutes by default; a whole pull is one request);
- Studio closes the link before answering.

A message larger than `server.max_message_mb` (64 MiB by default) closes the connection it came on, and the server's
log says so. A whole tree goes as one message.

The plugin runs each request apart from the link, so a long one doesn't hold up others. A request that fails in the
plugin comes back as `failed`, with the Luau error as the message.

The sync requests (`forge sync pull`):

| action | Fields | Reply |
|---|---|---|
| `classes` | `trees`: each tree's path from `forge.toml`, a list of names | `classes`: each class under the trees, with every property's `name`, `owner`, `engine` type, `serialized`, `read`, `write` and `deprecated`, from `ReflectionService` |
| `drafts` | `trees` | `drafts`: per tree, the full names of scripts whose `Source` differs from the editor's text (an uncommitted draft) |
| `watch` | `trees`, the read fields, `quietMs`, `pollMs` | `watching`; then the plugin sends `notify` with `{ tool: "sync", kind: "changed", tree: <index> }` whenever a tree has been quiet after a change |
| `unwatch` | | `watching: 0` |
| `clashes` | `lines` | `shown`: forge's window lists them |
| `push` | `trees`, `pushTrees` (each tree's operations, `tools/sync/src/push.rs`), `summary`, `lines`, `drafts`, `timeoutMs` | `result`: `applied`, `cancelled` or `timeout`; sent once Apply or Cancel is clicked in Studio |
| `read` | `trees`, `classes` (from the first reply), `properties` and `methods` (per class, each property's disk type), `models` (classes sent as a model file), `modelTimeoutMs`, `yieldEvery` | `trees`: per tree, `tree` (the instance tree in the JSON form `tools/sync/src/tree.rs` reads), `ms` (how long Studio took) and `drafts` (as above) |

### Matching places to repos

Every command says which repo it runs in (the nearest `forge.toml`), and the server remembers each one until it
stops. A Studio's repo is the remembered repo with the same place id. `forge status` shows each Studio with its repo,
and repos with no Studio open.

A place that was never published has place id 0 and belongs to no repo.
