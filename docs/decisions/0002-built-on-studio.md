# 0002 Built on Studio, its MCP and Script Sync; no Rojo, Argon or RbxSync

Accepted, 2026-11-11. Its Script Sync part is replaced by [0005](0005-sync-covers-code.md).

## Context

By late 2026 Roblox ships most of what the community tools were for:
- **Script Sync** (full release June 2026) syncs Script, LocalScript, ModuleScript and Folder two ways with a folder
  on disk. Every other instance is ignored, and a script with attributes or tags is skipped.
- **Studio's MCP server** lets agents build, inspect, run Luau (`execute_luau`), playtest, and call Open Cloud within
  per-game permissions (`open_cloud_invoke`).
- **Place branching and asset versioning** are on the roadmap for early 2027.

The options for the parts Script Sync leaves out (UI, maps, models):
- **Rojo** (7.x): patched by the community, no roadmap. Its `syncback` writes only binary `.rbxm`, not readable files.
- **Argon:** two-way sync of code only, no MCP.
- **RbxSync:** syncs every instance as JSON and has an MCP, but its licence allows evaluation only, its last Windows
  build is from January, and it needs `loadstring` on.
- **Our own tool** on Roblox's `SerializationService`, which plugins and MCP code can call.

## Decision

- Studio is the editor for everything; agents work through its built-in MCP; code moves with Script Sync.
- Rojo and Argon are not used. RbxSync is rejected for its licence.
- rbx-forge's first tool, `forge sync`, puts UI, maps and models in git as readable files and back, through
  `SerializationService` plus a disk-side reader (Lune or rbx-dom, picked by probe 2).
- Nothing is built before its five probes answer ([plan](../plan.md)).

## Consequences

- forge never touches scripts.
- The model-data format has no stability guarantee for other tools, so forge pins its reader and runs round-trip tests
  on every release.
- When Roblox's place branching covers the need, `forge sync` is dropped by a new record.

Sources:
- [Script Sync docs](https://create.roblox.com/docs/scripting/sync)
- [Script Sync full release](https://devforum.roblox.com/t/full-release-studio-script-sync/4688454)
- [Creator Roadmap 2026, fall update](https://devforum.roblox.com/t/creator-roadmap-2026-fall-update/4880208)
- [MCP: Open Cloud access and asset import](https://devforum.roblox.com/t/studio-mcp-server-updates-open-cloud-access-in-your-mcp-client-and-asset-import-workflows/4975849)
- [RbxSync](https://github.com/devmarissa/rbxsync)
