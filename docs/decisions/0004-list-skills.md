# 0004 Find Roblox's skills with `list_skills`, not a table in AGENTS.md

Accepted, 2026-11-11. Replaces the skill table from 0003 (reversed 2026-11-11, was "AGENTS.md lists which Roblox skill
to load for which job").

## Context

0003 put a table of Roblox's skills in AGENTS.md. The same day, the Studio MCP from the 11 November Studio build added
a `list_skills` tool, and the skills went from 7 to 17 (input, streaming, analytics, live configs, receipts, heap
profiling, network simulation and more). The table was out of date within hours.

## Decision

- Before a job in Studio, an agent calls `list_skills` and loads every skill whose description covers the job.
- AGENTS.md names no Roblox skills. The MCP is the only list.

## Consequences

- New Roblox skills are used as soon as Studio ships them, with no change here.
- An agent that skips `list_skills` works without Roblox's guidance; reviews check it was called.
