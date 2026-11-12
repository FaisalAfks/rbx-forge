# forge.toml and forge's settings

## A game's forge.toml

Each game's repo has a `forge.toml` at its root. It's the only place game-specific settings live; forge itself never
knows about one game. Commands find it the way git finds a repo: in the current folder or the nearest one above it.

```toml
# The place id, as Studio shows it.
place = 123456789012345

# One [[sync]] per part of the place to keep on disk.
[[sync]]
studio = "StarterGui.Hud"     # the instance's path in Studio, starting with a service
disk = "game/ui/hud"          # the folder it goes to, inside the repo

[[sync]]
studio = ["ReplicatedStorage", "Shared.v2"]   # a list of names, when a name has a dot in it
disk = "game/shared"
```

Rules, checked whenever forge reads the file:
- `place` is a number above 0.
- A Studio path names something inside a service, not the whole service.
- A disk path is a folder inside the repo: no `..`, no drive or leading `/`.
- No two entries overlap, in Studio or on disk, so no file is written from two places.
- Unknown keys are errors, so a typo never silently does nothing.

`forge config` prints the file forge found, with its place and trees.

## forge's own settings

Settings that belong to the PC, not to a game (the server's port, quiet times, the script poll), have defaults in
[`core/defaults.toml`](../core/defaults.toml) for forge's own and in each tool's `defaults.toml`
([sync's](../tools/sync/defaults.toml)), with a comment on each. To change one, put only that setting in the
settings file. On Windows, it's `%APPDATA%\forge\settings.toml`:

```toml
[server]
port = 35000
```

`forge config` names the file and prints every setting as forge will use it. A misspelled setting or a wrong type is an
error, not a silent default.
