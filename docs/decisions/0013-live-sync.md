# 0013 Live sync: merged per frame, compared by value, clashes held

Accepted, 2026-11-11. Answers probe 7 (#14).

## Context

`forge sync watch` keeps Studio and disk in step while Studio is open. It has to catch changes on both sides as they
happen, never echo its own writes back, and do something sensible when both sides change the same thing. The link is
forge's plugin and its WebSocket (decision 0012), and identity is the plugin's live map plus disk paths
(decision 0007).

## What was tested

**Studio side.** This ran through `execute_luau` (plugin rights) on a test folder in `ServerStorage`: 50 parts and a
ModuleScript. The folder was destroyed afterwards. Docs:
[ScriptEditorService](https://create.roblox.com/docs/reference/engine/classes/ScriptEditorService.md) and
[ScriptDocument](https://create.roblox.com/docs/reference/engine/classes/ScriptDocument.md).

| Test | Result |
|---|---|
| A flag set around forge's own write, to skip the echo | **Doesn't work.** Events are deferred: the handler ran after the flag was cleared and saw the write as a new change (1 of 1) |
| 50 parts moved every frame for 60 frames (a drag) | 3,000 writes fired **15,000** `Changed` events (`CFrame` plus its derived properties). Merged per frame: at most 250 property changes per frame, of which only `CFrame` is saved |
| `Changed` connected on every instance in the place | 3,926 of 3,926 in **5 ms** |
| A move between folders inside a watched tree | No `DescendantAdded` or `DescendantRemoving`; `Changed("Parent")` on the instance itself shows it |
| `UpdateSourceAsync` on a script with no open tab | **Nothing fires**: not `Changed`, not `TextDocumentDidChange` |
| An edit in an open tab (`EditTextAsync`) and `UpdateSourceAsync` with a tab open | `TextDocumentDidChange` fires for both |
| Reading every script's editor text (209 scripts, 1.1 MB) | **0.9 ms** |

**Disk side.** This used Python's `watchdog`, which on Windows uses the system's directory-change API, on a scratch
folder.

| Action | What the watcher sees |
|---|---|
| A plain save (most editors) | 2 × modified |
| A safe save (temp file, then rename over it) | created and modified on the temp file, deleted on the target, then a move |
| The same content written again | 2 × modified, for a file that didn't change |
| A folder renamed (an instance renamed) | one folder move, plus modified and attrib noise |
| `git stash` of 20 new scripts (like switching branches) | 60 events in one burst |

## Decision

**In Studio**, the plugin watches each synced tree:
- with `Changed`, `AttributeChanged`, `DescendantAdded` and `DescendantRemoving` on every synced instance (cheap: 5
  ms for the whole place);
- with `TextDocumentDidChange` for open script tabs;
- and with a poll of every synced script's editor text, once a second, for edits nothing announces (`UpdateSourceAsync`
  from the MCP, a collaborator's commit). At 0.9 ms for the whole place, that costs nothing noticeable.

Changes are **merged per frame** and filtered to saved properties (decision 0006), then **sent when the instance has
been quiet** for a short time, so a drag becomes one change at the end, not 15,000 messages. The quiet time is in
config.

**On disk**, the CLI watches the synced folders. It waits for a short quiet period, then reads the files that changed
as one batch, so a branch switch is one batch, not 60 events.

**Echoes are stopped by comparing values, not by flags.** Both sides keep the last synced state: a hash per file on
disk, and the last synced value per property in Studio.
- A change that brings a value back to what's already synced is dropped.
- That covers forge's own writes, editors that save twice, and files saved with the same content.
- Events arrive late (deferred), so a flag can't do this job.

**Clashes are held.** Each property is merged three ways, against the last synced state.
- Changes to different properties of the same instance both go through.
- When both sides change the same property to different values, **neither value is applied**. forge lists the clash
  with both values in `forge sync status`, and the plugin shows it in Studio, until a side is picked.
- Everything else keeps syncing.

**Writes into Studio** are one `ChangeHistoryService` recording per batch, so one Ctrl+Z undoes one batch from disk.

## Consequences

- The plugin needs a small state store (the last synced value per synced property) and a per-frame queue. The CLI needs
  the same per file. The quiet times and the poll interval live in config.
- A clash list is a visible part of the tool: in the CLI, in the MCP and in the plugin's widget.
- Moves that stay inside a synced tree arrive as `Changed("Parent")` and are matched through the live map (decision
  0007), so they become file moves, not a delete plus an add.
- These are the last of the plan's probes. The tool is built from decisions 0005 to 0013, in PR-sized steps.
