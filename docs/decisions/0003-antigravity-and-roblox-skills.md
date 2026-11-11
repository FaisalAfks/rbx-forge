# 0003 Antigravity reads `.agents/` itself; Roblox's skills come through the MCP

Accepted, 2026-11-11. Replaces the `GEMINI.md` part of 0001 (reversed 2026-11-11, was "a one-line `GEMINI.md` for
Gemini CLI and Antigravity").

## Context

0001 assumed Gemini CLI. The tools used here are Claude Code and the Antigravity IDE, which uses Gemini as its model.
Checked on 2026-11-11 in [Antigravity's rules docs](https://antigravity.google/docs/rules) and
[skills docs](https://antigravity.google/docs/skills):
- Workspace rules come from `AGENTS.md` or `GEMINI.md`, `.agents/AGENTS.md` or `.agents/GEMINI.md`, and
  `.agents/rules/*.md` (which need frontmatter, or they're dropped silently). Files over 24,000 bytes are cut.
- Workspace skills come from `.agents/skills/<name>/SKILL.md`.
- In Antigravity, `@path` doesn't inline a file; `@[label](path)` does.

Roblox's own skills (`rbx-debug`, `rbx-unit-test`, `rbx-docs-search`, ...) aren't files. Studio serves them through its
MCP: the agent calls the `skill` tool with a name and gets the current text. Roblox updates them with Studio.

## Decision

- No `GEMINI.md`. Antigravity reads `.agents/AGENTS.md` and `.agents/skills/` as they are.
- `CLAUDE.md` stays, one line: `@.agents/AGENTS.md`. Claude Code still needs the `.claude/skills` link.
- Roblox's skills stay with Roblox. AGENTS.md lists which one to load for which job, and agents load it through the MCP
  before starting. They're not copied into the repo, where they would go stale.

## Consequences

- One rules file and one skills folder serve both tools.
- An agent that skips the table works without Roblox's guidance; reviews check the right skill was used.
- If Roblox ships its skills as files, or Antigravity starts reading MCP skills itself, this record gets replaced.
