# rbx-forge: the plan

The home for Roblox tooling: **build tools, not just games.** Every tool has a CLI and an MCP, so agents can see and
use everything a game is made of. The first tool, **sync**, keeps a game's UI, maps and models in git as readable
files, next to its code (Studio's Script Sync handles code). Why it's built this way: [decision 0002](decisions/0002-built-on-studio.md).

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
- **`forge sync pull`:** reads chosen parts of the place (for example `StarterGui.Hud`, `Workspace.Maps.Arena`,
  `ReplicatedStorage.Models.Ember`) and writes each as a folder of readable files.
  - One JSON per object, with its class, its properties (typed: `UDim2`, `Color3`, `CFrame`, ...), attributes and tags.
  - Big meshes, images and sounds are referenced by asset id, not copied.
- **`forge sync push`:** turns those files back into objects and replaces the part in Studio. The plan is shown first
  ("3 changed, 1 added, 1 removed") and waits for an OK.
- **`forge sync diff`:** what differs between Studio and disk, as a readable list.
- **`forge.toml`** in the game's repo says which parts are synced and where they go on disk (`ui/`, `maps/`,
  `models/`).

**How it works:**

```
Studio ── plugin or MCP execute_luau ── SerializationService ──► model data
                                                                    │
forge CLI ◄─────────────────────────────────────────────────────────┘
  reads the model data, writes one readable file per object (and back)
```

- **Studio side:** `SerializationService:SerializeInstancesAsync` / `DeserializeInstancesAsync`, Roblox's own exporter
  and importer, available to plugins and to code run through the MCP, so every property Roblox saves is kept.
- **Disk side:** a library that reads Roblox model data.
  - **Lune** (`@lune/roblox`) is Luau, like the game, and ships a reflection database. Attributes and tags aren't in
    its docs.
  - **rbx-dom** (Rust) is the library inside Rojo. It's maintained, and handles attributes, tags and every type.
  - Probe 2 picks one of the two.
- **Getting the data out of Studio:** the plugin posts it to the forge CLI on `localhost`. That needs Studio's HTTP
  permission, limited to `localhost`. The MCP route returns it to the agent instead.
- **Caveat from Roblox's docs:** the model-data format has no stability guarantee for other tools. So forge pins one
  reader version, round-trip tests run on every release, and a Studio update that breaks the format shows up as a
  failing test, not as lost data.

**Safety:**
- A push never runs without a shown plan and an OK.
- A push keeps a backup of what it replaced (`.forge/backup/<time>/`).
- Scripts are never touched: Script Sync owns them.

## Probes first (no code until these answer)

Each is an issue on the board and ends in a decision record.

1. **Studio side:** does `SerializeInstancesAsync` run through the MCP's `execute_luau` and from a plugin? What's its
   size and time on a real screen and a real map?
2. **Disk side:** do Lune and rbx-dom both read that model data back with every property, attribute and tag intact?
   Checked on 3 real parts of anime-collectors: the HUD, a character rig, the arena.
3. **Readable format:** one file per object or one per part? How big and how readable is each, and how clear is a diff
   after a typical edit (a colour change, a moved frame, an added button)?
4. **Round trip:** Studio → disk → Studio, compared object by object, property by property.
5. **The localhost route:** the plugin's HTTP to the CLI, with the permission limited to localhost.

The tool is then built from the results, in PR-sized steps: the reader and the files, then pull, diff, push, the MCP,
and the plugin. It's built so it can be dropped once Roblox's place branching and asset versioning cover the need
(on the roadmap for early 2027).

## Afterwards (issues, not now)

Candidates for later, each only if it earns its place:
- `forge assets`: the manifest and upload through the MCP;
- `forge test`: the Studio CLI's RunScript, plus scenario playtests;
- `forge api`: a diff of `--fullApi` for each Studio update;
- `forge doctor`: Studio, MCP, permissions and Script Sync all set up right.
