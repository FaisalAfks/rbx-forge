# forge's tools: how the repo holds them

forge is one repo, one binary, one plugin and one version, holding many tools (decision 0019). Everything one tool is
lives in its own folder; what every tool uses lives in `core/`.

```
core/            forge-core: the link to Studio (server, client, protocol), settings, a game's forge.toml, the Tool interface
forge/           the forge binary: forge's own commands (config, serve, stop, status, mcp, plugin) and the list of tools
mcp/             forge mcp: serves every tool's MCP tools; knows none by name
plugin/          the plugin's shared parts: the link, the window, settings, the list of tools' Studio sides
tools/<name>/    one tool: its crate (src/, tests/), defaults.toml, studio/ (its Luau in the plugin), and its docs
```

## What a tool is

A tool's crate implements `forge_core::Tool` (`core/src/tool.rs`):

| Part | What it gives forge |
|---|---|
| `name` | its command (`forge sync`), its plugin folder (`tools/sync`) and the `tool` field of its requests |
| `command`, `run` | its subcommands and arguments (clap), and what runs them |
| `settings_defaults`, `settings_toml` | its sections of the settings file (its `defaults.toml`), read and checked by the tool |
| `game_keys`, `game_lines` | the keys of a game's `forge.toml` it owns, read and checked by the tool |
| `mcp_tools` | its MCP tools, each a forge command line built from its arguments |

Its Studio side is `tools/<name>/studio/main.luau`, a module with `handle(request, context)` and, optionally,
`describe(request, result)` for the line forge's window shows in Recent. The plugin packs every tool's `studio/` on its
own; `context` gives a tool what needs the plugin (`confirm`, `notify`, `showNotice`).

Core checks what no single tool can: a settings section or `forge.toml` key that no tool owns is an error, two tools
can't claim the same section, and two tools can't have MCP tools with the same name.

## Adding a tool

1. An issue on the board with its `Tool` set (add the tool to the board's Tool field and a `tool: <name>` label).
2. `tools/<name>/` with its crate implementing `Tool`, added to the workspace in `Cargo.toml`.
3. One line in each list: `TOOLS` in `forge/src/main.rs`, and `tools` in `plugin/main.luau` if it has a Studio side.
4. Its guide in `docs/<name>/` (what it reads, what it writes, what it never touches) and its skill in
   `.agents/skills/<name>/`.
5. A `### <name>` section in `CHANGELOG.md` under Unreleased.

## CI and releases

- Rust checks (`rust.yml`) run when Rust code, a tool's folder or the build changes. Luau checks (`luau.yml`) run when
  the plugin or a tool's `studio/` changes. A docs-only change runs only the PR rules.
- One version and one release for everything. Each release's changelog has a `### forge` section (core, the binary,
  the plugin, the MCP) and a section per tool that changed.
