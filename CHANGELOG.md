# Changelog

## Unreleased

## 0.1.1 - 2026-11-12

- `forge sync check` builds each tree's copy on its own: a tree plugins can't create (TextChatService's settings) is
  named and skipped, and the rest are checked, where it used to stop the whole check (#68). Check and push also keep
  new instances per tree, so a reference in one tree can't land on an instance another tree adds with the same id

## 0.1.0 - 2026-11-12

- The guide (`docs/sync/README.md`) takes a game from nothing to synced: install forge from the release, install the
  plugin, write `forge.toml`, pull, then work day to day, and what forge never touches. An agent skill
  (`.agents/skills/sync`) says when and how agents use it. Releases build `forge.exe` for Windows and attach it; the
  plugin is inside it (#41). Example ids in the docs and tests are made up

- `forge mcp`, an MCP server over stdio for agents (#40, `docs/mcp.md`), built on the Rust MCP SDK (`rmcp`). Its tools
  are status, pull, diff, push, check, clashes and pick; each runs the same forge command in the game's repo and
  returns what it printed. A push from it applies only after Apply in Studio, unless the person in Studio set forge
  not to ask

- `forge sync watch` keeps Studio and the files in step both ways until Ctrl+C (#38, #39, `docs/sync/watch.md`).
  The plugin says when a tree has gone quiet after a change in Studio, and forge watches the folders. Either way,
  both copies are merged three ways against the state they last agreed on: a change on one side goes to the other,
  and a value both sides changed differently is held as a clash, listed by `forge sync status` and forge's window,
  until `forge sync pick <n> studio|disk`. forge's own writes merge to nothing, so they don't echo. Link protocol 3

- forge's plugin has a button in Studio's Plugins tab and a proper window (#59): a home page with the link's state, a
  push waiting for Apply, and recent activity, with settings and a push's full plan as their own views. Settings live in
  Studio only: "Apply pushes" (ask every time, ask for large pushes, or never ask) and "Connect automatically". Applying
  without a click reverses part of decision 0015 (decision 0018); the plan is still printed and the backup still written

- `forge sync check [tree]` builds a copy of each tree in Studio from its files, reads the copy and compares it with
  the original value by value, then takes the copy away inside a cancelled undo recording (#37,
  `docs/sync/check.md`). Each difference is one line, and any difference fails the command

- `forge sync push` makes Studio match the files, after Apply is clicked in forge's window in Studio (#36,
  `docs/sync/push.md`). forge plans it in Rust: instances added, removed and moved, values set or reset, attributes,
  tags, script text and model files. The plugin shows the plan and applies it as one undo step, cancelled whole if
  anything fails; Studio keeps script text out of undo, so the backup is what puts pushed text back. What it replaces is backed up under `.forge/backup/<time>/`, and `forge sync push --from` puts a
  backup back

- `forge sync diff` lists what differs between Studio and the files, one line per change (#35, `docs/sync/diff.md`):
  instances added or removed (with how many below them), subtrees moved or renamed, changed properties, attributes
  and tags with both values, scripts with the lines added and removed, and changed model files. A command whose
  output is cut short (`| head`) now ends quietly instead of failing

- Pull lists scripts with an uncommitted draft, and `forge sync status` lists them without pulling (#20). A draft
  is found as a script whose `Source` (the last committed text) differs from the editor's text, since plugins can't
  read drafts. The files hold the editor's text; the warning says collaborators and the published game still get
  the committed one. Pull now shares its Studio session and tree reading with the commands to come

- `forge sync pull` (#34, `docs/sync/pull.md`) writes each tree in `forge.toml` from Studio to disk in layout C.
  - Studio lists the classes under the trees. forge then chooses which properties to read from Studio's own
    reflection data and `tools/sync/properties.toml`: saved and writable ones, twins, method-only values, and model
    files for MeshParts. Any saved property it can't sync is listed after the pull.
  - Only changed files are written. A second pull with nothing changed writes nothing, MeshPart model files
    included.
  - A `.gitattributes` in each folder keeps git from changing the bytes, checked on a clone with
    `core.autocrlf=true`.
  - Pull stops, writing nothing, for a folder forge didn't make, a file it doesn't write, or uncommitted changes it
    would overwrite (`--force` goes ahead).
  - The server now passes a command's request to the Studio on its place (link protocol 2). Pulling five real trees,
    689 instances, took 1.2 s

- forge's link to Studio (#33, `docs/link.md`):
  - `forge serve`, one per PC on 127.0.0.1, which any command that needs it starts in the background;
  - forge's plugin, which connects from Studio's edit session, tries again quietly when the server isn't there, and
    never prompts;
  - `forge status`, which lists linked Studios with their game repos;
  - `forge stop`;
  - `forge plugin install`, which packs `plugin/` and each tool's `studio/` into `forge.rbxmx`, with the plugin's
    settings as attributes.
  On Windows the server is started through WMI, so it doesn't hold the starting command's output pipes and a script
  reading that output isn't left waiting. A Luau workflow runs StyLua, selene and luau-lsp on the plugin, with the
  tools pinned in `rokit.toml`

- Sync's disk format, without Studio yet (#32). Every type Studio saves and Luau can read has a written form in
  `docs/sync/format.md`, with numbers that keep every bit and colours as `#rrggbb` only when exact. A tree turns into
  layout C and back:
  - names are safe on Windows and never clash with properties or forge's own files;
  - repeats get `~2`, and names read from disk are kept, so one change is one line;
  - references are paths;
  - script text keeps its exact bytes.
  JSON numbers are now parsed exactly (`serde_json`'s `float_roundtrip`), after a test caught Studio's 17-digit
  output reading back one bit off

- Probe #46 shows that forge can build a place without Studio. Model files written in Rust with rbx-dom loaded into
  Studio identical to the originals: 180 instances and 8,133 property reads, 0 differences. The Studio command line
  can't save a place and needs a login, so it isn't used for builds. `forge build` follows as #50 (decision 0017)

- Probe #45 finds that a plugin can capture Studio's viewport itself (`CaptureService`, then `EditableImage` for the
  pixels) in 0.6 s, and that a frame with a `UIScale` lays UI out as on any screen size. `forge ui shot` follows as
  #47. Capturing during a playtest wasn't tested and is probe #48 (decision 0016)

- The `forge` binary's first code (#31). `forge config` reads forge's per-PC settings and a game's `forge.toml`
  (the place id, and which parts of the place go to which folders). A bad file gives an error that names the
  problem. Settings default from `forge/defaults.toml`. A Rust workflow checks formatting, lint and tests on Windows
  and Linux, and Dependabot keeps the crates current

- The build plan for `forge sync`: twelve PR-sized steps, with the plugin and its link before pull. The CLI and MCP
  are Rust, after comparing Node, Python and Luau through Lune (#30, decision 0014). One `forge serve` per PC holds
  the plugin's link, forge packs its own plugin, and a push applies only after Apply is clicked in Studio (#30,
  decision 0015)

- Probe 7 settles live sync. Studio changes are merged per frame and sent once an instance is quiet; disk changes
  are read as one batch. Echoes are dropped by comparing values, because Studio delivers its events late. A clash on
  the same property is held until a side is picked (#14, decision 0013)

- Probe 5 picks the link between the CLI and Studio: a WebSocket from forge's plugin to the CLI on localhost, about
  one frame per round trip and 16 MB messages without trouble. Studio's MCP was ruled out as the transport because it
  cuts results at about 100 KB and can't send events (#7, decision 0012)

- Probe 4 shows the round trip Studio → disk → Studio is exact for GUI, map parts, scripts, attributes, tags and
  references. Floats are written at full precision, and MeshParts start from a small `.rbxm`, since their collision
  data can't be read and a rebuilt part came back up to 16% lighter (#6, decision 0011)

- Probe 9 keeps TOML for property files after comparing JSON, JSONC and YAML on real data: the smallest files, no
  comma noise, and every value read back exactly, where YAML read rig numbers like `-1e-06` back as text (#25,
  decision 0010)

- Probe 8 checks which Studio-only features a tool can reach. The debugger can be reached; Script Analysis can only
  take extra results; drafts and the Asset Manager can't. Stale script text comes from uncommitted drafts. Four
  tools follow as issues (#19, decision 0009)

- Probe 3 picks the disk layout: a folder only for instances with children, a `_.toml` per folder holding its own
  properties and its leaf children, and scripts as plain `.luau` files. Three layouts were compared on real data by
  file count and git diffs (#5, decision 0008)

- Probe 2 settles identity: forge writes nothing into the game. The plugin follows instances live through Studio's
  events, the disk path is an instance's identity, and references are written as paths. `UniqueId` can't be read by
  plugins and changes on every copy (#4, decision 0007)

- Probe 1 picks how Studio reads the tree: a Luau walk over saved properties with script text from
  `ScriptEditorService`. Roblox's binary export exported 55 of 110 scripts stale or empty, so it's kept only as a
  fallback for data Luau can't reach (#3, decision 0006)

- `forge sync` is planned to cover code too, as one nested tree with UI, maps and models; Script Sync is dropped,
  and the plan's probes now compare two designs built on Studio's current APIs (#13, decision 0005)
- AGENTS.md has agents call the Studio MCP's `list_skills` before a job instead of a fixed table of Roblox
  skills, which went from 7 to 17 in one Studio update (#10, decision 0004)
- No GEMINI.md: Antigravity reads `.agents/AGENTS.md` and `.agents/skills/` itself, and AGENTS.md lists which of
  Roblox's MCP skills to load for which job (#8, decision 0003)
- The repo's setup (#1): rules in `.agents/`, issue forms that put issues on the board, the `pr-rules` check, a
  release workflow for SemVer tags, the plan and decisions 0001 and 0002
