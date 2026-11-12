# Packages: Wally and pesde

forge doesn't install packages (decision 0021). `wally install` (or `pesde install`) puts them in `Packages/`, and a
packages tree puts that folder into Studio the way Rojo did, instance for instance.

## Setting it up

In `forge.toml`, one more `[[sync]]` with `kind = "packages"`:

```toml
[[sync]]
studio = "ReplicatedStorage.Packages"
disk = "Packages"
kind = "packages"
```

The tree's top has to exist in Studio, as for every tree: a game that used Rojo already has it, otherwise make an
empty Folder with that name. Then:

```bash
wally install
forge sync push
```

`Packages/` usually stays out of git: `wally.toml` and `wally.lock` are what's committed, and an install rebuilds it.

## Disk owns it

| Command | On a packages tree |
|---|---|
| `pull` | leaves it alone, and says so |
| `diff` | lists what differs between `Packages/` and Studio |
| `push` | makes Studio match `Packages/` |
| `watch` | pushes when `Packages/` changes, and puts back an edit made to it in Studio; never writes into `Packages/` |
| `check` | builds a copy from `Packages/` in Studio and compares it with the files |

A push writes a package's script text as committed text (`Source`), not as a draft, so every collaborator and every
`require` gets it at once, as with Rojo. A draft someone left on a package script is replaced too.

## What forge reads

The part of Rojo's format packages use, following Rojo 7.7:

- A folder is a Folder. One with an init file is that script: `init.luau` or `init.lua` a ModuleScript,
  `init.server.luau`/`.lua` a Script, `init.client.luau`/`.lua` a LocalScript. Two init files in one folder is an error.
- `Name.lua` and `Name.luau` are ModuleScripts, `Name.server.lua(u)` Scripts, `Name.client.lua(u)` LocalScripts.
  Other dots stay in the name (`World.spec.luau` is `World.spec`).
- A folder with a `default.project.json` is only what the project says. Its `tree` holds `$path` alone, a folder or a
  script file inside `Packages/`, and the instance takes the project's `name`, or the folder's when it has none.
  Everything else beside the project file (tests, READMEs, `wally.toml`) isn't part of the package.
- Files Rojo skips are skipped (`.md`, licences).

Anything else Rojo would turn into an instance is an error that names the file, so a package forge can't read whole
never lands in Studio different from Rojo's: JSON, TOML, text, CSV and model files, `.meta.json`, `.plugin.lua`, other
`.project.json` files, and project keys beyond `name` and `tree` with `$path`.
