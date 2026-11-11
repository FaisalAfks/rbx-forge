# 0005 `forge sync` covers code too; Script Sync is dropped

Accepted, 2026-11-11. Replaces the Script Sync part of 0002 (reversed 2026-11-11, was "code moves with Script Sync;
forge never touches scripts").

## Context

0002 left code to Studio's Script Sync and gave forge everything else. Two things count against that split:
- Script Sync looks like a side project for Roblox: it syncs only scripts and folders, skips scripts with attributes or
  tags, can only be started by hand from the Explorer, and Roblox's own docs still point to Rojo for a whole project
  on disk. A game would need two sync tools with two sets of rules.
- A script is an instance like any other: a class, properties (`Source` among them), attributes, tags and children.
  The Explorer nests instances freely, scripts under parts, parts under scripts. One tree on disk, with code in it,
  follows the game as it is.

Building a Rojo-like tool used to be hard because Studio exposed little. Checked on Studio 0.747 through the MCP's
`execute_luau`, which runs with plugin rights:

| Need | What Studio has now | Result |
|---|---|---|
| Every property of every class, without shipping a database | `ReflectionService:GetPropertiesOfClass` | Works: each property's type, owner class, `Serialized` flag and read/write permits (60 on `Frame`) |
| Exact model data, every property kept | `SerializationService:SerializeInstancesAsync` / `DeserializeInstancesAsync` | Available (probe 1 measures it) |
| Script text as the editor has it | `ScriptEditorService:GetEditorSource`, `UpdateSourceAsync` | Works. `Script.Source` came back empty for 26 of 145 scripts that the editor shows with text, so `Source` can't be trusted |
| Undo for every change made in Studio | `ChangeHistoryService:TryBeginRecording` | Available |
| A live link to a process on the PC | `HttpService:CreateWebStreamClient` (WebSocket), HTTP | Available; HTTP is on for this game |
| Change events | `DescendantAdded`, `DescendantRemoving`, `Changed`, `AttributeChanged` | Available |
| A stable id per instance | `Instance.UniqueId` | **Not readable** from plugins ("lacking capability RobloxScript"); the MCP's own `inspect_instance` shows it. Whether the model data carries it is for probe 2 |
| What Script Sync tracks | `InstanceFileSyncService` | Read-only for plugins: nothing can start a sync from code |

## Decision

- `forge sync` syncs every instance, scripts included, as one nested tree on disk. Studio stays the editor; git holds a
  readable copy that can also be edited and pushed back.
- Script Sync isn't used.
- The design starts from these APIs, not from Rojo or Argon. Their ideas are used only where they're the only good
  option, and the decision record says so.
- Script text is read and written only through `ScriptEditorService`.

## Consequences

- Probes 3, 5 and a new probe 7 change: a nested layout that holds scripts, the link between Studio and the CLI, and
  live two-way changes.
- Identity across renames and moves needs an answer: `UniqueId` from the model data, or forge's own id.
- forge now owns the code's safety too: every push shows its plan, keeps a backup and can be undone in Studio.
