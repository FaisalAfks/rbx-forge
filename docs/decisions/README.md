# Decisions

Architecture decision records: one short file per decision, with its context, the decision and what follows from it.
A decision is changed by a new record that replaces the old one, never by editing history. Each probe ends in one.

| # | Decision | Date |
|---|---|---|
| [0001](0001-rules-in-agents-folder.md) | Rules in `.agents/`, with one-line CLAUDE.md and GEMINI.md (GEMINI.md dropped by 0003) | 2026-11-11 |
| [0002](0002-built-on-studio.md) | Built on Studio, its MCP and Script Sync; no Rojo, Argon or RbxSync (Script Sync dropped by 0005) | 2026-11-11 |
| [0003](0003-antigravity-and-roblox-skills.md) | Antigravity reads `.agents/` itself; Roblox's skills come through the MCP (skill table replaced by 0004) | 2026-11-11 |
| [0004](0004-list-skills.md) | Find Roblox's skills with `list_skills`, not a table in AGENTS.md | 2026-11-11 |
| [0005](0005-sync-covers-code.md) | `forge sync` covers code too; Script Sync is dropped | 2026-11-11 |
| [0006](0006-read-with-luau.md) | Studio reads the tree with Luau (candidate A); model data only where Luau can't reach | 2026-11-11 |
| [0007](0007-identity-outside-the-game.md) | Identity is kept outside the game: live objects in Studio, paths on disk | 2026-11-11 |
| [0008](0008-layout-folders-where-needed.md) | Disk layout: folders only where there are children | 2026-11-11 |
| [0009](0009-studio-only-features.md) | Studio-only features: what a tool can reach (drafts, lint, debugger, assets) | 2026-11-11 |
| [0010](0010-property-files-toml.md) | Property files are TOML, after comparing JSON, JSONC and YAML | 2026-11-11 |
| [0011](0011-round-trip.md) | Round trip is exact; MeshParts start from a small model file | 2026-11-11 |
| [0012](0012-plugin-websocket-link.md) | The CLI and Studio talk over a WebSocket held by forge's plugin | 2026-11-11 |
| [0013](0013-live-sync.md) | Live sync: merged per frame, compared by value, clashes held | 2026-11-11 |
| [0014](0014-cli-in-rust.md) | The CLI and MCP are written in Rust | 2026-11-11 |
| [0015](0015-how-forge-runs.md) | One forge server per PC, a packed plugin, pushes confirmed in Studio | 2026-11-11 |
| [0016](0016-ui-capture.md) | Seeing UI: forge stages a screen, captures it itself, and crops it | 2026-11-11 |
| [0017](0017-build-without-studio.md) | Places are built without Studio, by writing Roblox's file format in Rust | 2026-11-11 |
| [0018](0018-apply-pushes-setting.md) | When a push asks for Apply is a setting in Studio (reverses part of 0015) | 2026-11-12 |
