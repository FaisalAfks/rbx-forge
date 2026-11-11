# 0001 Rules in `.agents/`, with one-line CLAUDE.md and GEMINI.md

Accepted, 2026-11-11.

## Context

The repo is worked on with several coding tools. darbit and ad-bot keep one `AGENTS.md` at the root and no
tool-specific files (darbit decision 0004). For the Roblox repos the rules move into `.agents/`, so everything the
tools read sits in one folder: `AGENTS.md`, `rules/` and `skills/`.

What each tool reads, checked on 2026-11-11:
- Claude Code reads `CLAUDE.md` and follows `@path` imports in it; it reads skills only from `.claude/skills/`.
- Gemini CLI reads `GEMINI.md` and follows `@path` imports in it, relative or absolute
  ([GEMINI.md docs](https://geminicli.com/docs/cli/gemini-md/)); it reads skills from `.agents/skills/`.
- Antigravity reads a root `AGENTS.md` and `.agents/rules/` (darbit decision 0004), not `.agents/AGENTS.md`.

## Decision

- `.agents/AGENTS.md` holds the rules, the map of the repo and the commands.
- `CLAUDE.md` and `GEMINI.md` at the root are one line each: `@.agents/AGENTS.md`.
- `.agents/skills/<name>/SKILL.md` holds the how-tos; `scripts/setup-dev.ps1` links `.claude/skills` to it (a
  directory junction, git-ignored).
- `.agents/rules/` is added only when a rule must load for certain files.

## Consequences

- One place to update the rules for every tool, as in darbit, but under `.agents/`.
- A tool that reads only a root `AGENTS.md` (Antigravity, Codex, Copilot) has to be pointed at `.agents/AGENTS.md`.
  If one of them is used here regularly, an always-on rule in `.agents/rules/` that points to it is the fix.
- A new clone runs `scripts/setup-dev.ps1` once.
