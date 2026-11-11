---
name: new-task
description: Start work on an rbx-forge issue, with its branch, board status and PR.
---

# Start a task

1. Read the issue and the part of `docs/plan.md` it belongs to.
2. Check the work it depends on is merged, and its probes have decision records. If not, stop and say so.
3. Move the issue to In Progress on the rbx-forge board.
4. Branch from the latest `main`: `feature/<issue>-<short-name>` (or `fix/…`, `docs/…`, `chore/…`).
5. Keep the tool general: nothing in it knows about one game; game settings go in that game's `forge.toml`.
6. Test what can be tested without Studio in CI; check the Studio side through the MCP, after looking at the screen.
7. Update `CHANGELOG.md` (Unreleased) and the docs the change touches; add a decision record for a real fork in the
   road.
8. Open the PR: what changed, why, `Closes #<issue>`, in the owner's voice. Merge (squash) once its checks pass.
