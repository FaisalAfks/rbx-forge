# 0007 Identity is kept outside the game: live objects in Studio, paths on disk

Accepted, 2026-11-11. Answers probe 2 (#4).

## Context

forge has to recognise an instance after a rename or a move, and has to write references (a weld's parts, a model's
`PrimaryPart`) so they point at the right instance. Probe 1 showed that full paths can't do it: two instances can
share one full name, and a path-written reference then lands on the wrong one (decision 0006).

The probe's other question was whether Lune and rbx-dom read Roblox's model data back. That half was dropped by 0006,
because forge doesn't parse model data.

Docs used:
- [Instance.UniqueId](https://create.roblox.com/docs/reference/engine/classes/Instance.md): readable only with
  `RobloxScriptSecurity`; saved and loaded with the place.
- [ChangeHistoryService](https://create.roblox.com/docs/reference/engine/classes/ChangeHistoryService.md)

## What was tested

These ran on Studio 0.747 through the MCP. A test folder went into `ServerStorage` and was removed afterwards, and the
place wasn't saved. `UniqueId` was read with the MCP's `inspect_instance`, the only place it shows.

**`UniqueId`**

| Action | Id |
|---|---|
| Rename and move to another folder | Same |
| Delete, then undo | Same |
| Saved place, reopened in a later session | Kept: the arena's id has a different session prefix than the new test part's |
| `Clone()` | New |
| `SerializeInstancesAsync` then `DeserializeInstancesAsync` | New |
| Read from plugin Luau (`execute_luau`) | Error: "lacking capability RobloxScript" |

There's no call that finds an instance by its `UniqueId`. `inspect_instance` takes a path, one per call, and returns
every instance on an ambiguous path.

**Studio's events** (what a plugin sees): renaming fired the `Name` change, and moving and deleting fired
`AncestryChanged` and `DescendantRemoving`. Undoing a recording that renamed, moved and deleted a part fired the events
back in reverse. The instance that came back was the same Luau object the plugin held before the delete.

**forge's own id as an attribute** (a 12-character `forge_id` on every instance of an unparented copy):

| Target | rbxm before | After | Growth |
|---|---|---|---|
| HUD (96) | 14,955 B | 16,546 B | +11% |
| Arena (84) | 5,888 B | 7,195 B | +22% |
| Shared (68) | 124,233 B | 125,290 B | +1% |

`Clone()` copied the id onto 96 of 96, 84 of 84 and 68 of 68 instances. Game code sees attributes, and they replicate
to clients. In anime-collectors, `Net/Packet` reacts to every attribute change on its remote and treats the name as a
packet name.

**References in the place:**
- 134 saved references: 120 `Motor6D` `Part0`/`Part1`, 10 `InputAction.PreferredBinding` and 4 `Model.PrimaryPart`.
  Other references exist too, but Studio computes them and doesn't save them.
- 68 instances share a full name with another one.

## Options compared

1. **`UniqueId`.** It survives renames, moves, undo and saves. But plugins can't read it, nothing looks it up, and
   copies get a new one. Not usable.
2. **forge's id as an attribute on every instance.** It survives everything, even edits made without forge. But it
   costs:
   - up to 22% more data;
   - something every game script and client can see;
   - duplicates on every copy;
   - a first sync that writes into every instance of the game.
3. **Ids only on referenced instances.** The same costs, on about 130 instances.
4. **Identity kept outside the game.** Nothing is written into the place:
   - while Studio is open, the plugin holds the instances themselves and follows renames and moves through events;
   - on disk, an instance's path is its identity;
   - between sessions, a sync state file links the two.

Option 4 was chosen.

## Decision

- **Nothing forge-specific is written into the game.** No id attributes or tags.
- **In Studio:** the plugin keeps a map from each synced instance (the Luau object) to its disk path. Renames, moves,
  deletes and undo arrive as events and update the map and the files.
- **On disk:** an instance's path in the layout is its identity. Probe 3's layout makes every path unique, including
  for siblings that share a name.
- **References** are written as the target's disk path. A rename rewrites the references that point at it, in the same
  change.
- **Between sessions:** `.forge/state` (git-ignored) keeps the last synced tree as paths with a fingerprint of each
  instance. Fingerprints use class, saved properties and children.
- **Changes forge didn't see** (Studio edited while the plugin wasn't running, or a Team Create collaborator without
  it): on the next pull they're matched by fingerprint. A match is a rename or move; no match is a delete plus an add.
  Either way the push or pull plan shows it before anything is written, so a wrong match is visible, not silent.

## Consequences

- Probe 3 needs a layout where every path is unique and stable, including siblings with the same name, and has to show
  how a reference to a path looks in a file.
- Probe 7 owns the live part: the event handling, and the plugin keeping its map while the CLI isn't connected.
- A rename made outside forge can show up as a delete plus an add when the content changed too. That's the price of
  writing nothing into the game, and the push plan exists for it.
- If Roblox opens `UniqueId` to plugins, or adds a lookup by it, this record gets replaced.
