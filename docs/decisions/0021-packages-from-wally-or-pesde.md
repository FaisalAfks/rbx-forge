# 0021 Packages come from Wally or pesde; forge only puts them in Studio

Accepted, 2026-11-12. Part of #70. Work in #66.

## Context

Roblox games get shared Luau code (ECS libraries, UI libraries, networking) as packages. A game that drops Rojo for
forge still needs its packages in Studio.

What a Wally install looks like, from anime-collectors:
- `Packages/Matter.lua` is a link module: `return require(script.Parent._Index["matter-ecs_matter@0.8.5"]["matter"])`;
- `Packages/_Index/<scope>_<name>@<version>/<name>/` holds the package itself, and each package is a small Rojo
  project: a `default.project.json` such as `{"name": "matter", "tree": {"$path": "lib"}}`, with `.lua` and `.luau`
  files and `init.lua` for folders.

The options compared:
- **Build a package manager.** A registry, hosting, version resolution and every community package would have to be
  rebuilt. Nothing forge does needs that.
- **Cargo.** It's Rust's package manager: it builds forge itself, but can't fetch Luau packages or read Wally's
  registry.
- **Use Wally or pesde as they are, and have forge read what they install** (chosen).
  - Wally is what anime-collectors uses. Its last release is v0.3.2 (June 2023), but it works and its repo still gets
    commits.
  - pesde is active (v0.7.4, September 2026), has its own registry and installs Wally packages too. It also lets any
    sync tool plug in through a `roblox_sync_config_generator` script, so forge doesn't have to be Rojo to work with
    it.

## Decision

- **forge never installs, resolves or hosts packages.** `wally install` or `pesde install` does that.
- **A packages tree is owned by disk:** forge pushes it into Studio and never pulls into it, so a package's files
  are never rewritten from Studio.
- **forge reads the small part of Rojo's format that packages use:** a nested `default.project.json` with `$path`,
  `init.lua` and `init.luau`, and `.lua` files. It doesn't read general Rojo projects; moving a whole Rojo game is #67.
- **pesde support** comes through the same reader. A forge `roblox_sync_config_generator` script for pesde's
  registry is a later issue if a game uses pesde.

## Consequences

- #66 adds the disk-owned tree and the package reader; it needs #72 so packages and a game's code share file names.
- Switching from Wally to pesde later changes nothing in forge.
