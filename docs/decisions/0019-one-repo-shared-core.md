# 0019 One repo, a shared core, and one interface every tool fills in

Accepted, 2026-11-12. Part of #70. Work in #71.

## Context

forge started as one tool (sync) and now has more on the board: lint (#21), debug (#22), assets (#23), UI capture
(#47) and building places without Studio (#50). The plan already gives each tool a `tools/<name>/` folder
(`docs/plan.md`), but the code doesn't follow it everywhere yet:
- sync's commands (`forge/src/sync.rs`, `watch.rs`) live in the binary, not in `tools/sync`;
- the link to Studio, the protocol, sessions and settings are in the binary too, so a second tool would either reach
  into `forge/` or copy them;
- the binary, `forge mcp` and the plugin each list sync by hand in their own way.

Two shapes were compared:
- **A repo per tool**, with the shared parts published as a library. Each tool releases on its own, but every tool
  talks to Studio over the same protocol and through the same plugin, so a protocol change means a release of every
  repo in the right order, and a person installs several binaries and plugins that have to match.
- **One repo with a shared core** (chosen). One binary, one plugin, one version; a protocol change lands in one PR
  with every tool that uses it.

## Decision

- **`core/`** is a crate holding what every tool uses: the link (server and client), the protocol, sessions with
  Studio, settings and `forge.toml` reading. It knows no tool by name.
- **`tools/<name>/`** holds everything one tool is: its Rust crate, its `studio/` Luau, its docs and its tests.
- **One interface, `Tool`,** that each tool's crate fills in: its commands, its MCP tools and its Studio actions.
  The binary, `forge mcp` and the plugin each keep one list of tools, and adding a tool is one line in each.
- **CI** runs a tool's checks when its folder or `core/` changes; changes to `core/` run everything.
- **One version and one release** for everything. The changelog gets a section per tool inside each release, so a
  release still says what changed in each.
- **The board** already has a Tool field and `tool:` labels; every issue sets one.

## Consequences

- #71 moves the code into this shape before more tools land, while it's still a move and not a rewrite. It changes
  no behaviour: v0.1.1's tests and a live pull, push and watch are the check.
- A tool that turns out to need its own release can still leave later; nothing here stops that.
