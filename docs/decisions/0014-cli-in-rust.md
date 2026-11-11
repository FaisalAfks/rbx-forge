# 0014 The CLI and MCP are written in Rust

Accepted, 2026-11-11. Part of the build plan (#30).

## Context

The probes are answered (decisions 0005 to 0013), and `forge` needs a language for the CLI, its server and its MCP.
The plugin is Luau either way. What the CLI side has to do, from those decisions:
- run a WebSocket server on `127.0.0.1` for the plugin (0012);
- watch folders on Windows and wait for quiet (0013);
- read and write TOML with every float exact (0010, 0011);
- serve an MCP over stdio (every tool has one);
- install on the owner's PC and in CI without fuss.

The probes' throwaway scripts were Python. That says nothing about the tool; they were written to be thrown away.

## What was tested

On this PC (Windows 11):
- start-up time of a program that reads one property file, median of 10 runs;
- whether each language's float output reads back bit for bit (the TOML part checked with Rust's `toml_edit` and
  Python's `tomllib`);
- `execute_luau` calls to check what Studio itself sends: `HttpService:JSONEncode` on infinity, NaN, `-0`, 2,708 real
  and random numbers, and 10,600 sums of decimals like `0.1 + 0.2`, each read back with a correct parser (Python's).

Cells marked *docs* come from the library's documentation, not a test here. Lune isn't installed on this PC.

| | Rust 1.99 | Node 24 (TypeScript) | Python 3.14 | Luau (Lune) |
|---|---|---|---|---|
| Start-up | **9 ms** | 49 ms | 166 ms, before any libraries | not installed |
| Floats read back exactly | yes (`-0.0` kept) | **no**: `String(-0)` is `"0"`, so `-0` needs special code | yes | not tested |
| Shortest float text | needs a formatter: `toml_edit` wrote `5e-324` as 320 digits (exact but unreadable) | yes | yes | not tested |
| File watching | `notify` crate (*docs*) | `fs.watch`, recursive on Windows (*docs*) | `watchdog` (tested in probe 7) | **none**: Lune's `fs` has no watch function (*docs*) |
| WebSocket server | `tokio-tungstenite` (*docs*) | `ws` (*docs*) | `websockets` (*docs*) | `net.serve` (*docs*) |
| MCP SDK | `rmcp`, the official Rust SDK (*docs*) | the reference SDK (*docs*) | the official SDK (*docs*) | none (*docs*) |
| TOML that keeps comments and layout when edited | `toml_edit` (*docs*) | not checked | `tomlkit` (*docs*) | not checked |
| Installing | one `.exe`, no runtime | Node and `npm` | Python and `uv` | the Lune binary |
| Same language as the plugin | no | no | no | **yes** |

What Studio sends:
- `JSONEncode` writes 17 digits, with an odd last digit (`0.1 + 0.2` comes out as `0.30000000000000007`, where
  `tostring` gives `0.30000000000000004`). 1,348 of the 10,600 decimal sums looked off like that. It's still exact:
  the odd digit stays within half a step of the true value, so a correct parser read every one of the 13,308 numbers
  back as the same value.
- Infinity and NaN come out as a wrapped object (`{"t":"numeric","v":"inf"}`), not a number. That shape isn't
  documented, so the plugin writes them itself.
- `-0` comes through as `-0`.

## Decision

**Rust.**
- **Install:** one `.exe` with no runtime, for the owner's PC, CI and other games' repos. Release builds go on the
  GitHub releases.
- **The pieces are standard crates:** the official MCP SDK, a native file watcher, a WebSocket server, and a TOML
  library that can edit in place if that's ever needed.
- **Exact values:** strict types for the 30-odd Roblox value types are the best guard for an exact round trip
  (0011). Floats keep `-0`.
- **Start-up** is the smallest reason. `watch`, the server and the MCP run for a long time anyway. Still, agents run
  `forge sync diff` often, and 9 ms against 166 ms adds up.

**Why not the others:**
- **Luau through Lune** would share one language with the plugin. It can't watch files, though, and has no MCP SDK,
  so both would be written by hand.
- **TypeScript** has the reference MCP SDK. But it loses `-0` without care, and it needs Node installed wherever
  `forge` runs.
- **Python** was fine in the probes. But it's the slowest to start and needs Python and `uv` installed. Its types are
  checked only by a separate tool.

**Floats:** the CLI writes the shortest text that reads back as the same number (the `ryu` algorithm). It never
writes Studio's 17 digits or `toml_edit`'s long form. Infinity and NaN are TOML's `inf` and `nan`, and the plugin
sends them as strings.

## Consequences

- CI builds and tests on Windows and Linux: `cargo fmt`, `clippy` with warnings as errors, and `cargo test`.
- Logic lives on the Rust side wherever it can: the layout, names, diff, the three-way merge and the clash list
  (0013). Rust is tested in CI, and Studio can't be. The plugin stays thin: it reads, writes, watches and drops echoes.
- Building from source needs Rust (`rustup`). People who only use `forge` take the release `.exe`.
