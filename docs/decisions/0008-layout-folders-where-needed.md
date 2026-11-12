# 0008 Disk layout: folders only where there are children

Accepted, 2026-11-11. Answers probe 3 (#5).

## Context

forge writes a game as one nested tree on disk (decision 0005). Anything can sit under anything, so the layout has to
handle:
- scripts under parts and parts under scripts;
- siblings that share a name;
- references, written as disk paths (decision 0007).

The layout shouldn't copy Rojo or Argon unless it wins on its own merits.

## What was tested

Studio exported four trees with decision 0006's reader: the old HUD (96 instances), the arena (84),
`ReplicatedStorage.Shared` (68, 56 of them scripts) and the Ember rig (99, with its `Motor6D` references). The JSON
was posted over HTTP to a local receiver. That needed `127.0.0.1:34900` in the MCP's HTTP allowlist: the entry has to
include the port, and the bare IP wasn't enough.

A throwaway Python script wrote the four trees in three layouts and committed each to git. It then replayed six edits
and recorded `git diff -M`. The script is on the issue. The rules shared by all three layouts:
- property files are TOML, with only the saved properties that differ from their default;
- colours are `#rrggbb` when exact;
- script text is plain `.luau`;
- a sibling with a name already taken gets `~2`, `~3` (case-insensitive, so it works on Windows);
- characters that aren't allowed in Windows or Unix file names are `%XX`-escaped;
- references are `"@<path>"`.

The three layouts:
- **A, a folder for every instance:** each one gets `instance.toml`, and a script also gets `source.luau`.
- **B, one outline file per tree:** sections are keyed by path, and the scripts are mirrored as `.luau` files.
- **C, folders only for instances with children:** each folder's `_.toml` holds its own properties and its leaf
  children as sections. Scripts are `<name>.luau` beside their siblings.

| | A | B | C |
|---|---|---|---|
| Files / folders | 403 / 347 | 60 / 12 | 145 / 89 |
| Colour of one frame | 1 line | 1 line | 1 line |
| Move a frame holding 57 instances | 57 renames | 311 lines | 20 renames, 0 lines |
| Add a button with a `UICorner` | 2 new files | 10 lines | 1 new file, 8 lines |
| Rename a script | 2 renames | 1 rename, 1 line | 1 rename, 1 line |
| Move a script under a part | 2 renames | 1 rename, 2 lines | 1 rename, new folder, 30 lines |
| Rename a rig part that joints point at | 11 renames, 3 lines | 30 lines | 1 rename, 3 lines |

The comparison was shown as a rendered page, and C was chosen.

## Decision

**Layout C.**
- **Folders:** an instance with children is a folder, which matches what's expanded in the Explorer.
- **The folder's `_.toml`** (a plain Folder needs none since decision 0022): the instance's class, its properties, attributes and tags, and one section per child that
  has no children of its own.
- **Scripts:**
  - a script's text is always `<name>.luau` in its parent's folder;
  - its class and attributes sit in its parent's `_.toml` (a leaf) or in its own folder (a script with children).
  - There's no class suffix in file names.
- **Names:** each sibling's file name is unique: `~2` and up for repeats, and `%XX` for characters file systems
  refuse. A `name =` line keeps the real name when the file name differs.
- **References:** written as `"@<path>"`. A rename rewrites them in the same change.
- **Values:** only saved properties that differ from their default (decision 0006). Colours are `#rrggbb` when exact,
  enums are their item name, and other types are plain arrays. Exactly how each type is written is settled in the
  first build PR, from ReflectionService's types, and documented there.
- **Sibling order isn't stored.** Neither the file system nor TOML sections keep it, and in these trees layout comes
  from `LayoutOrder`, not from child order. If a game needs it, it becomes an option.

## Consequences

- **Promotion churn:** when a leaf gets its first child, it moves from its parent's `_.toml` into a folder of its own,
  which is a bigger diff (the "script under a part" row). The reverse happens when it loses its last child. That's
  accepted in exchange for a quarter of A's folders and no 300-line diffs on a move.
- **Repeated names:** `~2` numbering follows sibling order. Deleting the first of two same-named siblings renames the
  second's file. The plugin's live map (decision 0007) still knows which instance it is, so only the file name moves.
- Probe 4 (round trip) runs on this layout, and so does the first build PR.
