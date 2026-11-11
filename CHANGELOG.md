# Changelog

## Unreleased

- `forge sync` is planned to cover code too, as one nested tree with UI, maps and models; Script Sync is dropped,
  and the plan's probes now compare two designs built on Studio's current APIs (#13, decision 0005)
- AGENTS.md has agents call the Studio MCP's `list_skills` before a job instead of a fixed table of Roblox
  skills, which went from 7 to 17 in one Studio update (#10, decision 0004)
- No GEMINI.md: Antigravity reads `.agents/AGENTS.md` and `.agents/skills/` itself, and AGENTS.md lists which of
  Roblox's MCP skills to load for which job (#8, decision 0003)
- The repo's setup (#1): rules in `.agents/`, issue forms that put issues on the board, the `pr-rules` check, a
  release workflow for SemVer tags, the plan and decisions 0001 and 0002
