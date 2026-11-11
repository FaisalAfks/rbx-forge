# rbx-forge: the plan

The home for Roblox tooling: **build tools, not just games.** Every tool has a CLI and an MCP, so agents can see and
use everything a game is made of. The first tool, **sync**, keeps a whole game in git as
readable files: code, UI, maps and models, as one nested tree. Why it's built this way: decisions
[0002](decisions/0002-built-on-studio.md) and [0005](decisions/0005-sync-covers-code.md).

## The layout, once there's code

```
rbx-forge/
  forge/           the CLI: one command, a subcommand per tool (forge sync ..., forge doctor ...)
  mcp/             one MCP server that exposes every tool to agents
  plugin/          one Studio plugin: the Studio side of every tool (export, import, status)
  tools/<name>/    each tool's own code, docs and tests
  docs/            the plan, decisions/, a guide per tool
  .agents/         AGENTS.md, skills/ (one skill per tool, so agents know when and how to use it)
```

Rules that keep it a toolbox, not a game:
- A tool never knows about one game. Game-specific settings live in the game's repo (a `forge.toml`).
- Every tool works three ways, so agents, scripts and people all use the same thing:
  - from the CLI;
  - from the MCP;
  - from Studio, when it has a Studio side.
- Each tool says in its doc what it reads, what it writes, and what it never touches.

## Tool 1: sync

**What it does:**
- **`forge sync pull`:** reads chosen parts of the place (for example `ServerScriptService.Server`, `StarterGui.Hud`,
  `Workspace.Arena`) and writes each as a tree of readable files on disk.
  - Every instance is kept: its class, its properties (typed: `UDim2`, `Color3`, `CFrame`, ...), attributes, tags and
    children. A script is an instance like any other, with its text in a `.luau` file next to it.
  - Nesting is kept as the Explorer shows it, whatever is under what: scripts under parts, parts under scripts.
  - Big meshes, images and sounds are referenced by asset id, not copied.
- **`forge sync push`:** applies the files back to Studio. The plan is shown first ("3 changed, 1 added, 1 removed")
  and waits for an OK; the change is one undo step in Studio.
- **`forge sync diff`:** what differs between Studio and disk, as a readable list.
- **`forge sync watch`:** keeps both sides in step while Studio is open, once probe 7 says how.
- **`forge.toml`** in the game's repo says which parts are synced and where they go on disk.

**How it could work** (two candidates; probe 1 picked A, with B only as a fallback for data Luau can't reach,
decision 0006):

- **A. Studio writes the readable form itself.** The plugin walks the tree, lists each class's saved properties with
  `ReflectionService`, reads script text with `ScriptEditorService`, and sends plain JSON and Luau. The CLI only writes
  files. No library has to read Roblox's model data, and the property list always matches the running Studio.
- **B. Studio sends model data, the CLI converts it.** `SerializationService:SerializeInstancesAsync` gives Roblox's
  own binary format, with every saved property; a library on the CLI side (Lune's `@lune/roblox` or rbx-dom) turns it
  into readable files and back. Every property is kept by construction, but the format has no stability guarantee for
  other tools, so the reader is pinned and round-trip tests run on every release.

What's settled either way:
- Script text goes through `ScriptEditorService` (`Script.Source` came back empty for 26 of 145 scripts, and stale
  for more; decision 0006).
- Studio changes are recorded with `ChangeHistoryService`, so a push is one undo step.
- The design doesn't copy Rojo or Argon. Where one of their ideas is used, it's because it's the only good option,
  and its decision record says so.

**Safety:**
- A push never runs without a shown plan and an OK.
- A push keeps a backup of what it replaced (`.forge/backup/<time>/`).

## Probes first (no code until these answer)

Each is an issue on the board and ends in a decision record.

1. **Studio side:** how fast and how big are candidate A's walk and candidate B's `SerializeInstancesAsync` on a real
   screen, a real map and the game's code, run through the MCP's `execute_luau` and from a plugin? **Answered by
   decision 0006:** A; B loses script text and is kept only as a fallback.
2. **Identity:** how does forge know an instance after a rename or a move, and how are references written: `UniqueId`
   in the model data (plugins can't read it), or an id of forge's own? (The Lune and rbx-dom half was dropped by
   decision 0006.) **Answered by decision 0007:** nothing is written into the game; the plugin follows live objects,
   the disk path is the identity, and references are written as paths.
3. **Readable layout:** how nested instances and scripts look on disk, with every path unique (siblings can share a
   name) and references written as paths. At least two layouts compared on the HUD, the arena and a code folder: size, readability, and the diff after a colour change, a moved frame, an added button, a
   renamed script and a script moved under a part. **Answered by decision 0008:** folders only for instances with
   children; leaf children are sections in the folder's `_.toml`; scripts are `.luau` files.
4. **Round trip:** Studio → disk → Studio, compared object by object, property by property, scripts included.
5. **The link:** how the CLI and Studio talk. A plugin with a WebSocket or HTTP to `localhost`, against the CLI calling
   Studio's MCP server itself. What each needs (permissions, Studio settings), and its speed and size limits.
6. ~~Script Sync for code~~: dropped by decision 0005.
7. **Live changes:** catching changes on each side as they happen (Studio's change events, file watching), without
   loops, and what happens when both sides change the same instance.

The tool is then built from the results, in PR-sized steps: the readable layout, then pull, diff, push, watch, the MCP
and the plugin.

## Afterwards (issues, not now)

Candidates for later, each only if it earns its place:
- `forge assets`: the manifest and upload through the MCP;
- `forge test`: the Studio CLI's RunScript, plus scenario playtests;
- `forge api`: a diff of `--fullApi` for each Studio update;
- `forge doctor`: Studio, the MCP, permissions and the plugin all set up right.
