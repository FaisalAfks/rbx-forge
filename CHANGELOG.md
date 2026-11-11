# Changelog

## Unreleased

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
