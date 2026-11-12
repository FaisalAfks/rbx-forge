# forge sync push

Makes Studio match a game's files. Run it in the game's repo with the place open in Studio. **Nothing changes until
Apply is clicked in forge's window in Studio** (decision 0015): the command, an agent or the MCP can propose a push,
but only a click where the place is open applies it.

```bash
forge sync push                                      # the game's own files
forge sync push --from .forge/backup/20261112-075106  # a backup: puts back what an earlier push replaced
```

## What happens

1. forge reads Studio the same way pull does (`pull.md`), and the files the same way diff does (`diff.md`).
2. It works out the plan (`tools/sync/src/push.rs`): instances added, removed and moved, properties set or reset to
   their default, attributes, tags, script text and model files. The command prints it, with a summary such as
   `2 changed, 1 added, 5 removed, 0 moved`.
3. It writes what Studio has now to `.forge/backup/<time>/`, laid out like the game. `.forge/` gets a `.gitignore`, so
   backups never land in git.
4. Unless forge's "Apply pushes" setting says not to ask (below), forge's window opens in Studio with the push waiting:
   its summary, Show what changes (the full plan), and Apply and Cancel. Scripts with an uncommitted draft that the push
   writes over are listed in a warning colour.
5. **Apply** carries out the plan as one change in Studio's undo history. If any part fails, the whole change is
   cancelled and Studio is as it was. **Cancel**, closing the window, or `sync.confirm_timeout_ms` (10 minutes)
   passing changes nothing, and the backup is removed.

When Studio already matches, push says so and opens nothing.

## Apply pushes: the setting

In forge's window, under Settings (decision 0018):

| Choice | A push |
|---|---|
| Ask every time (the default) | waits for Apply |
| Ask for large pushes | waits only above the number of changes set there (20 to start); smaller ones apply at once |
| Never ask | applies at once |

The setting is stored in Studio, so only someone in Studio can change it, never a command, an agent or a file.
Without a click, the plan is still printed, the backup is still written, and the push shows in the window's Recent list.

## How it applies a plan

- Instances are matched by disk path, as in diff. A subtree found whole under another path is moved, not removed and
  added, so it stays the same instance.
- New instances are built from the files first, unparented, so references between them can be set before any appears
  in the place. MeshParts start from their model file (decision 0011).
- A property the files don't have is set back to its default (the value a new instance of the class has).
- A path whose class changed is replaced whole.
- Removed instances are unparented, not destroyed, so undo can bring them back.
- Script text goes in through `ScriptEditorService` only (decision 0005).

## Undo and backups

One Ctrl+Z in Studio takes back the whole push: values, added, removed and moved instances.

**Script text isn't part of that undo.** Studio keeps edits made through `ScriptEditorService` outside its undo
history, so after Ctrl+Z a pushed script keeps its new text. The backup has the old text: `forge sync push --from
<backup>` puts it back. With collaborative editing, the pushed text is a draft until it's committed in Studio's
Drafts window, like any edit.

## What it never touches

- Anything outside the synced trees.
- Properties pull doesn't sync (`pull.md`): `BaseScript.RunContext` can't be written by plugins, so a script push
  adds keeps Studio's default, and an existing script keeps its own.
- The files: push only reads them.
