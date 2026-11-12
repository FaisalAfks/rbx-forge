---
name: sync
description: Work on a Roblox game that uses forge sync - read and change it through its files and forge's commands (status, pull, diff, push, check, watch) instead of editing Studio directly. Use in any game repo with a forge.toml.
---

# Working on a game with forge sync

A game repo with a `forge.toml` keeps parts of its place as files (`docs/sync/README.md` in rbx-forge). Work on the
files and move changes with forge; don't edit those parts in Studio through other tools while forge owns them.

## Before anything

1. `forge sync status` (MCP: `status`): is the game's Studio linked? Any held clashes? If not linked, the place isn't
   open in Studio or the plugin isn't installed: tell the person, don't try to open Studio yourself.
2. `forge sync diff` (MCP: `diff`): do Studio and the files agree? If they differ, ask which side is right before
   pulling or pushing over the other.

## Making a change

1. Edit the files: `.luau` for scripts, `_.toml` for everything else (`docs/sync/format.md` has every value's form).
   Keep a script's line endings as they are.
2. `forge sync diff`: check the change is only what you meant.
3. `forge sync push` (MCP: `push`): it shows the plan in Studio and waits for Apply, unless the person set forge not
   to ask. **Read the result line**: applied, cancelled, or timed out. Never report a push as done without
   "applied".
4. `forge sync check <tree>` after structural changes, to confirm the tree survives the round trip.

## Taking Studio's changes

`forge sync pull` writes Studio's trees to the files. It stops on uncommitted changes it would overwrite: commit or ask,
don't `--force` unless the person said so.

## Rules

- Never pick a side of a clash (`forge sync pick`) on your own: show both values and ask.
- Never change forge's settings in Studio, and never click Apply for the person unless they said so for that push.
- Scripts with an uncommitted draft: what's on disk is the editor's text; collaborators get the committed text until
  the person commits the draft in Studio's Drafts window. Mention it when a script you touch has one.
- `.forge/` holds backups and watch's state; it's never committed.
