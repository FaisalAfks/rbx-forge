# AGENTS.md

How to work in this repo. Read by Claude Code through `CLAUDE.md`, by Antigravity directly
([decision 0003](../docs/decisions/0003-antigravity-and-roblox-skills.md)), and by people. Longer guides live in
`docs/`; this file points to them.

## What this is

rbx-forge: the home for Roblox tooling. **Build tools, not just games.** Every tool has a CLI and an MCP, so agents,
scripts and people all use the same thing, and a Studio plugin side where it needs one. The first tool is **sync**:
a whole game in git as readable files, code included, as one nested tree.

What's built next and why is in [docs/plan.md](../docs/plan.md); why things are the way they are is in
[docs/decisions/](../docs/decisions/README.md).

| Path | What |
|---|---|
| `docs/` | the plan, decisions, and later a guide per tool |
| `scripts/` | dev helpers (`setup-dev.ps1`) |
| `.agents/` | this file and `skills/`: step-by-step how-tos (new task, probe, review, release) |

The tool folders (`forge/`, `mcp/`, `plugin/`, `tools/<name>/`) come with the first code, after the probes.

## Commands

```bash
powershell -File scripts/setup-dev.ps1   # once per clone: the .claude/skills link and the pre-commit hooks
```

## Rules

**Git and GitHub**
- Never commit or push to `main`. One branch per task (`feature/<issue>-<name>`, `fix/…`, `probe/…`, `docs/…`,
  `chore/…`), and a PR into `main` that says `Closes #<issue>`.
- GitHub checks these: pre-commit refuses a commit on `main`; the `pr-rules` check fails a PR without
  `Closes #<issue>` or a `CHANGELOG.md` line (label `skip-notes` when none is needed); issues opened from the forms go
  on the board.
- Every change has an issue on the **rbx-forge** board (FaisalAfks project 2), with its `Tool` set. Move it to
  In Progress when work starts; it closes only when its PR is merged. A new idea becomes an issue, not a note in a doc.
- Merge a PR (squash) once its checks pass. Never on red, and never re-run a check to get past a flaky failure.
- Releases: SemVer tags `vX.Y.Z` on `main`, with a GitHub release and short notes (`.agents/skills/release`). Never
  move or delete a tag.
- Commit as the owner: `git -c user.name=FaisalAfks -c user.email=fmehmadi9@gmail.com commit`.

**Writing** (commits, PRs, issues, releases, changelog, docs, comments)
- In the owner's voice: plain, minimal, simple.
- Never "I", "we" or "the user"; nothing about who asked for what.
- Never mention AI, assistants or agents in anything public. No `Co-Authored-By` or "Generated with" lines.
- Describe in prose what changed and why it matters to the code.

**A toolbox, not a game**
- A tool never knows about one game. Game-specific settings live in the game's repo (a `forge.toml`).
- Every tool works from the CLI, from the MCP, and from Studio when it has a Studio side.
- Each tool's doc says what it reads, what it writes, and what it never touches.
- Script text is read and written only through `ScriptEditorService`; `Script.Source` can be empty when the editor
  isn't (decision 0005).

**How work is done**
- Research before building: check how Studio, a library or an API really behaves (its docs, a probe) before building
  on it. A probe is an issue (form "Probe") and ends in a decision record (`.agents/skills/run-a-probe`).
- No code for a tool until its probes answer. Probe scripts stay in the probe's issue or decision record, not in the
  repo's tool folders.
- Compare at least two options before building anything, including what Roblox already ships, and say why one won.
- Plans are complete and researched before they're shown.
- A decision that turns out wrong gets a new record that replaces it ("Reversed <date>, was X"); old records are never
  rewritten.
- Modular and finished: extend what exists before adding a parallel one, and never half-do a feature.
- Configure, don't code: a number that changes lives in config.
- Readable code: small functions with clear names, a short summary at the top of each file, comments that say why.
- No stub files for future work: future work is an issue.
- Keep files' line endings (LF in the repo, see `.gitattributes`).

**Studio and the owner's PC**
- Agents work in Studio through its built-in MCP. One Studio per agent (`studio_id`), never two agents on one Studio.
- Roblox's own skills come only through the MCP. Before a job in Studio, call `list_skills` and load (`skill`) every
  one whose description covers the job. Roblox adds and changes them with Studio updates, so the list is never copied
  here ([decision 0004](../docs/decisions/0004-list-skills.md)).
- Look at the screen before any live action (clicks, keys, starting or stopping Studio or a playtest). If the owner
  might be using the PC, or the state isn't what was left, ask.
- A push into Studio never runs without a shown plan and the owner's OK, and keeps a backup of what it replaced.
- Never create, copy or paste API keys or passwords. Open Cloud goes through the MCP's `open_cloud_invoke`, within the
  per-game permissions the owner set.
- Never spend Robux, upload or publish without the owner's word for that one thing.
- Never commit secrets or private game data.

**Asking**
- Ask questions as multiple choice whenever the answer is a choice. Keep messages short, one topic at a time.
