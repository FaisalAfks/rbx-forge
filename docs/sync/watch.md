# forge sync watch

Keeps Studio and a game's files in step both ways while it runs (decision 0013). Run it in the game's repo with the
place open in Studio; Ctrl+C stops it.

```bash
forge sync watch
forge sync status            # while it runs: clashes it holds, numbered
forge sync pick 1 studio     # settle clash 1: keep Studio's value (or `disk` for the files')
```

It starts only when Studio and the files agree (`forge sync diff` says so): run `pull` or `push` first. That agreed
state is what every later change is merged against.

## What it watches

- **In Studio**, through the plugin: the properties pull reads, attributes, names, instances added, removed and moved,
  script text as it's typed, and a poll of every synced script (`watch.script_poll_ms`, 1 s) for edits nothing
  announces, such as another plugin's `UpdateSourceAsync`. When a tree has been quiet for `watch.studio_quiet_ms`
  (250 ms), the plugin tells forge, once: a drag is one change, at its end.
- **On disk**: the synced folders. When a folder has been quiet for `watch.disk_quiet_ms` (300 ms), its tree is handled
  as one batch, so a branch switch or an editor saving several files is one change.

Tags aren't watched: Studio has no event for them on an instance. A tag change shows up with the next change in its
tree, and `forge sync diff` always sees it.

## What happens on a change

The tree is reconciled. forge reads Studio's copy and the files, and merges each value three ways against the state they
last agreed on (`tools/sync/src/merge.rs`):

| Studio | Files | Result |
|---|---|---|
| changed | as agreed | the files get Studio's value (only changed files are written) |
| as agreed | changed | Studio gets the files' value, as a push: one undo step, asking for Apply as forge's setting says (`push.md`) |
| changed | changed the same way | agreed; nothing moves |
| changed | changed differently | **a clash**: held; neither side is changed |

Different properties of one instance merge separately, so both sides' edits go through. An instance added or removed on
one side follows that side; removed on one side and changed on the other is a clash.

**Echoes don't happen.** forge's own writes come back as changes (the file watcher sees its writes, the plugin sees its
pushes), and they merge to nothing, since the values already agree. That's comparing values, not flags (decision
0013), so an editor saving the same bytes twice changes nothing either.

## Clashes

A clash stays held until it's settled. Everything else keeps syncing meanwhile, other values in the same instance and
file included. Clashes show:
- in watch's output as they happen;
- in `forge sync status`, numbered;
- in forge's window in Studio, in red on its home page.

`forge sync pick <number> studio|disk` settles one: the running watch takes that side's value and sends it across.

## Checked on a real place

| What | Result |
|---|---|
| A frame moved 30 times in a row in Studio | one file written, after it stopped |
| A script added in Studio, then edited with `UpdateSourceAsync` (no event) | both reached disk, the edit within the poll |
| Two files edited | one push into Studio with both, and nothing came back to disk |
| The same bytes saved twice | nothing at all |
| The same property changed both ways | held as a clash, shown in status and the window; a script edited meanwhile still synced |
| `forge sync pick 1 disk` | Studio took the files' value; the clash list emptied |
