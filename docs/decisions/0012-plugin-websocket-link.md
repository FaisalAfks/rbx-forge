# 0012 The CLI and Studio talk over a WebSocket held by forge's plugin

Accepted, 2026-11-11. Answers probe 5 (#7).

## Context

forge's CLI writes files, and Studio reads and writes instances (decision 0006). Something has to carry trees, pushes
and, later, live changes between them. The two candidates:
- **A plugin** that keeps a WebSocket (`HttpService:CreateWebStreamClient`) or HTTP link to the CLI on `localhost`.
- **The CLI as an MCP client** of Studio's own `StudioMCP.exe`, running Luau through `execute_luau`, with no plugin of
  its own.

Docs: [HttpService.CreateWebStreamClient](https://create.roblox.com/docs/reference/engine/classes/HttpService.md) and
[WebStreamClient](https://create.roblox.com/docs/reference/engine/classes/WebStreamClient.md). The WebSocket client is
Studio-only, four clients at most, and text messages only.

## What was tested

A throwaway WebSocket server on `127.0.0.1:34900` played the CLI. It ran a fixed test on each client:
- 20 pings;
- messages of 1 KB to 16 MB in both directions;
- exports of the HUD and the arena;
- a push of the HUD tree read back from disk.

The client code was the same in two places:
- **through the MCP's `execute_luau`**, which runs inside Roblox's assistant plugin and needs the address in the MCP's
  HTTP allowlist;
- **as a local plugin file** in `%LOCALAPPDATA%\Roblox\Plugins`. Studio loads new plugin files only at start-up, so
  Studio was restarted from the command line (`RobloxStudioBeta.exe -task EditPlace -placeId … -universeId …`). The
  plugin connected about 10 seconds later, with no prompt.

A throwaway Python MCP client then started `StudioMCP.exe` over stdio, alongside the two already running, and timed
its calls. Scripts and output are on the issue.

## Results

| | WebSocket, through `execute_luau` | WebSocket, local plugin | CLI as MCP client |
|---|---|---|---|
| Round trip | 16.6 ms median (one Studio frame) | 43 ms median, measured while Studio was still loading | 67 ms per `execute_luau` call |
| 16 MB to Studio / from Studio | 167 / 201 ms | 210 / 212 ms | sending 1 MB of code: 96 ms. **Results are cut at about 100 KB** (64 KB came back whole; 1, 4 and 16 MB came back as 100,015 characters) |
| Export HUD / arena | 18 / 14 ms | 9 / 13 ms | not measured (capped by the 100 KB limit) |
| Push of the HUD tree (27 KB) | 16 ms | 15 ms | |
| Needs | the address in the MCP's HTTP allowlist (with the port) | the plugin file, and a Studio start | nothing extra |
| Side effects | each call is an "Assistant" undo step | none | each call is an "Assistant" undo step |
| Studio → CLI without being asked (events) | no; runs only when called | yes, the plugin can send any time | no; the CLI has to poll |

Other notes:
- `studio_id` changes on every Studio start.
- The restart kept all 73 uncommitted drafts (decision 0009).
- Studio's console is full of errors from its own Drafts widget ("Expected draft to be Instance. Got 'nil'"). Those
  are Roblox's, not forge's.

## Decision

- **forge's plugin holds the link.** It's a WebSocket client that connects to the CLI on `127.0.0.1` and reconnects
  when the CLI starts. The CLI is the server, and messages are JSON.
- It's fast enough that a whole tree moves in one message (16 MB tested). It's the only option that lets Studio tell
  the CLI about changes as they happen (probe 7). It needs no MCP permissions, and it leaves no undo steps of its own.
- **The MCP path isn't the transport.** The 100 KB cap would split every real tree, and it can't push events. It stays
  the way agents drive Studio, and forge's own MCP server talks to the CLI, not to StudioMCP.
- **Installing the plugin** means copying it into the Plugins folder and starting Studio. `forge` gets an install
  command that does the copy and says Studio has to restart. Starting Studio from the CLI works with the place id.

## Consequences

- Probe 7 builds on this link: Studio's change events go to the CLI as messages.
- The plugin has to cope with the CLI not running: it tries quietly, never prompts, and never blocks Studio.
- Port, retry timing and message limits go in config, not code.
