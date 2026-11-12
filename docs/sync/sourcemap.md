# Sourcemap: luau-lsp and selene

luau-lsp follows `require(ReplicatedStorage.Shared.Util)` and `script.Parent.x` through a sourcemap: a
`sourcemap.json` naming each instance, its class and the file behind it, in the format Rojo made standard.
`forge sync sourcemap` writes one from the game's files.

```bash
forge sync sourcemap                         # sourcemap.json in the game's folder
forge sync sourcemap --output other.json     # somewhere else
luau-lsp analyze --sourcemap=sourcemap.json --platform=roblox --definitions=globalTypes.d.luau src
```

It reads only the files, so it needs no Studio and no `forge serve`, and runs the same in CI. Run it again after
adding, moving or renaming a script; an editor can run it on every change (luau-lsp's VS Code extension:
`"luau-lsp.sourcemap.generatorCommand": "forge sync sourcemap"`).

## What's in it

- The game is a `DataModel` named after the game's folder, with `forge.toml` as its file.
- Each tree in `forge.toml` sits under its Studio path. File paths are from the game's folder, with `/`.
- Scripts point at their `.luau` file (decision 0020). Packages trees point at what `wally install` made, read the
  way [packages.md](packages.md) describes.
- Files a tree's `ignore` patterns match aren't read.
- Only scripts and the instances that lead to them are listed, as Rojo does by default. `--non-scripts` lists every
  instance (Rojo's `--include-non-scripts`).

**Classes without Studio.** The files give every class inside a tree. Above a tree's top, forge only has the path:
- a service's class is its name (`ReplicatedStorage`);
- the containers Roblox makes under a service keep their class (`StarterPlayer.StarterPlayerScripts` and
  `StarterPlayer.StarterCharacterScripts`);
- anything else between a service and a tree's top is a Folder (`ReplicatedStorage.Modules` for a tree at
  `ReplicatedStorage.Modules.Camera`).

Properties aren't in a sourcemap, so it never needs Studio's schema.

## Checked

CI writes the map for a small game in `fixtures/sourcemap-game` and runs luau-lsp on it: it has to fail without the
map, since the requires can't resolve, and pass with it. On anime-collectors the map lists the same 254 instances,
classes and script files as Rojo's, apart from files Rojo reads that forge doesn't (JSON modules, `.meta.json`, a
package's `default.project.json`).
