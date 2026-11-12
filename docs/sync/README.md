# forge sync: the guide

forge sync keeps a Roblox game in git as readable files: scripts as `.luau`, everything else (UI, maps, models) as
small TOML files, one folder per instance that has children. It reads and writes Studio through forge's own plugin,
and changes nothing in Studio without your say-so.

This page takes a game from nothing to synced. Each command's page has the details: [pull](pull.md), [diff](diff.md),
[push](push.md), [check](check.md), [watch](watch.md), and the file format in [format.md](format.md).

## 1. Install forge (once per PC)

1. Download `forge.exe` from the latest [release](https://github.com/FaisalAfks/rbx-forge/releases), and put it in a
   folder on your `PATH` (or run it by its full path).
2. Install the plugin into Studio, then restart Studio (it loads plugins only when it starts):

   ```bash
   forge plugin install
   ```

   In Studio's **Plugins** tab there's now a **forge** button. It opens forge's window: the link to forge, a push
   waiting for Apply, recent activity, and settings.

Nothing else needs starting: any command that needs `forge serve`, the small server the plugin talks to, starts it in
the background. `forge status` shows it and the Studios linked to it.

## 2. Tell forge about the game (once per game repo)

In the game's repo, write a `forge.toml` ([forge-toml.md](../forge-toml.md)): the place's id, and which parts of the
place go to which folders.

```toml
place = 1234567890  # yours: Studio, File > Game Settings > Basic Info

[[sync]]
studio = "StarterGui.Hud"
disk = "game/hud"

[[sync]]
studio = "ReplicatedStorage.Shared"
disk = "game/shared"
```

The place id is in Studio (File > Game Settings > Basic Info, or `game.PlaceId`). Each `studio` path starts with a
service; each `disk` folder is inside the repo, and no two overlap.

## 3. Pull

Open the place in Studio, then, in the repo:

```bash
forge sync pull
```

Each tree is written to its folder. Commit them. A second pull with nothing changed writes nothing. Pull lists
properties plugins can't sync and scripts with an uncommitted draft (collaborators get the committed text until a draft
is committed in Studio).

## 4. Day to day

| To | Run | Changes |
|---|---|---|
| see if Studio is linked, drafts, held clashes | `forge sync status` | nothing |
| see what differs between Studio and the files | `forge sync diff` | nothing |
| bring Studio's changes into the files | `forge sync pull` | the files |
| bring the files' changes into Studio | `forge sync push` | Studio, after Apply |
| keep both in step while you work | `forge sync watch` (Ctrl+C stops) | both |
| check a tree survives Studio -> files -> Studio | `forge sync check` | nothing |

**Push** shows its plan in forge's window in Studio and waits for **Apply**. In the window's **Settings**, *Apply
pushes* can ask every time (the default), only for large pushes, or never. Only someone in Studio can change that.
Every push first backs up what it replaces in `.forge/backup/<time>/`; `forge sync push --from <that folder>` puts it
back.

**Watch** merges both ways: a change on one side goes to the other. When both sides change the same value differently,
it's held as a clash (in `forge sync status` and forge's window) until you pick: `forge sync pick <number> studio` or
`disk`.

## What forge never touches

- **Anything outside the trees in `forge.toml`.** forge reads and writes only those.
- **Studio, without a push you approved** (or the setting you chose). Pull, diff, status and check change nothing in
  Studio.
- **Files forge didn't write.** A synced folder with files forge doesn't write stops a pull rather than losing them.
- **Uncommitted changes.** A pull that would overwrite them stops instead (`--force` goes ahead).
- **What plugins can't reach.** A few saved values (a script's `RunContext`, some place-wide flags) stay as they are
  in the place file, and pull lists the ones in your trees.

## For agents

`forge mcp` serves these commands as MCP tools ([mcp.md](../mcp.md)). An agent can pull, diff, check and propose a
push; the push still waits for Apply in Studio unless you set it not to.
