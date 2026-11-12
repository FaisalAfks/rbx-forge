# forge sync pull

Writes each tree named in a game's `forge.toml` from Studio to its folder, in layout C (`format.md`). Run it in the
game's repo, with the place open in Studio and forge's plugin installed. It starts `forge serve` if it isn't running
(`../link.md`).

```bash
forge sync pull           # every [[sync]] tree in forge.toml
forge sync pull --force   # see "What stops a pull"
```

For each tree it prints how many instances it read, how many files it wrote, removed and left alone, and how long Studio
and the disk took:

```
ServerStorage.Hud -> game/hud: 96 instances; 33 written, 0 removed, 0 unchanged (read 1 ms, write 25 ms)
```

## What it reads

From the Studio that has the place open (one only):
- **every instance under each tree** that Studio saves. An instance with `Archivable` off isn't saved by Studio, so it
  and everything under it are left out;
- **saved properties** that differ from their default and that a plugin can both read and write, and aren't
  deprecated (decision 0006);
- **twins**: a saved property Luau can't read, through the readable one that holds the same value (`Color3uint8`
  through `Color`, `size` through `Size`, and so on);
- **values only a method reaches**, such as a Model's scale through `GetScale`;
- **attributes and tags**;
- **script text, only through `ScriptEditorService`**: the text in the editor, not `Script.Source` (decisions 0005, 0009);
- **a model file of each MeshPart alone**, without its children (decision 0011). It carries the mesh and collision data
  a plugin can't read or write. Each one has `sync.model_timeout_ms` to come back.

Which property goes which way is in `tools/sync/properties.toml`, applied to the running Studio's own reflection data.
So a Studio update that adds a property is picked up without a forge update.

**Nothing is dropped without a word.** A saved property none of the above covers, and that isn't listed in
`properties.toml` as left in the place file (with why), is listed after the pull:

```
not synced, because plugins can't read and write them (they stay as they are in the place file):
  BaseScript.RunContext (Enum)
```

**Scripts with an uncommitted draft are listed after the pull.** With collaborative editing, an edit stays a draft
until it's committed in Studio's Drafts window. Until then the script's `Source` keeps the last committed text,
which is what collaborators and the published game get, while the editor (and so the file on disk) has the draft.
Plugins can't read or commit drafts, so forge finds them by comparing the two and says so (#20):

```
32 scripts have an uncommitted draft. forge has the editor's text for them, but collaborators and the published game
get the last committed text until each draft is committed in Studio's Drafts window:
  ReplicatedStorage.Shared.Net.Remotes
```

`forge sync status` lists the same scripts without pulling, and says whether the game's Studio is linked.

## What it writes

- **Each tree's folder**, in layout C. Only files whose bytes changed are written, so a second pull with nothing changed
  in Studio writes nothing. Files of instances no longer in Studio are removed, and so are the folders that leaves empty.
- **A `.gitattributes` in each folder** with `* -text`. git then keeps every file byte for byte, whatever
  `core.autocrlf` or the repo's own `.gitattributes` say. Script text keeps its exact line endings and last newline
  through a commit and a fresh clone.

Every folder's changes are worked out and checked before any file is written. A pull that stops changes nothing.

Before that, each tree's files are read back: they have to hold every instance Studio sent, and writing what was read
has to give the same bytes. That's decision 0011's round trip, run on every pull on the real data.

## What it never touches

- **The place.** Pull only reads from Studio.
- **Files forge doesn't write** in a synced folder (anything but `_.toml`, `.luau` and `.rbxm`) and dotfiles. Those are
  never removed.
- **Anything outside the synced folders.**

## What stops a pull

Each of these stops the pull with an error that says what to do. `--force` goes ahead anyway.

| What | Why | With `--force` |
|---|---|---|
| The folder has files but no `_.toml` | forge didn't make it, so it could be someone's own code | written over; files shaped like forge's (`.toml`, `.luau`, `.rbxm`) are replaced or removed |
| The folder has a file forge doesn't write | pull won't remove it, and layout C has no place for it | left where it is |
| A file pull would change or remove has uncommitted changes in git | they'd be lost | written over |

Files git doesn't track yet (an earlier pull nobody committed) don't stop it.

These also stop it, and aren't forced:
- no Studio with the place, or more than one;
- a tree path that names nothing, or names two siblings with the same name;
- the tree changing in Studio between pull's two requests (run it again);
- the files not reading back whole (a bug in forge: the error names the tree and the file);
- git failing for any reason other than not being installed or the folder not being in a repo.

## How long it takes

Measured on a real place, in Studio:

| Tree | Instances | Studio's read | Disk write |
|---|---|---|---|
| A HUD | 96 | 1 ms | 25 ms |
| A map | 84 | 1 ms | 15 ms |
| Code, 56 scripts | 68 | 1 ms | 38 ms |
| Code, 31 scripts | 40 | 1 ms | 22 ms |
| Assets, with 56 MeshParts | 401 | 945 ms | 76 ms |

Model files are most of the time: about 17 ms per MeshPart. The whole pull of the five trees took 1.2 s.
