# Changelog

## Unreleased

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
