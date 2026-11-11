# 0017 Places are built without Studio, by writing Roblox's file format in Rust

Accepted, 2026-11-11. Answers probe #46.

## Context

Rojo and Argon can build a place or model file from disk with no Studio running. That's what lets a game's CI build
a release file and run checks on it. forge reads and writes through a running Studio (decisions 0006 and 0012), so
the question was whether forge can build without one, and how.

Two ways were compared:
- **A. The Studio command line.** `RobloxStudioBeta.exe --task RunScript --runScriptFile … --quitAfterExecution`
  opens a place, runs a script and quits ([docs](https://create.roblox.com/docs/studio/command-line-interface)).
- **B. Writing the file directly** with rbx-dom's Rust crates (`rbx_dom_weak`, `rbx_binary`, and
  `rbx_reflection_database` for property types), the library Rojo uses. Decision 0006 kept it only as a fallback for
  *reading* Studio. This is about *writing* files.

## What was tested

**A, from the docs only.** The command line wasn't run. Starting a second Studio is live input on the owner's PC, and
the screen couldn't be checked first. The docs show:
- RunScript writes only the script's output (`--outputFile`). No flag saves the place, and there's no documented
  headless mode.
- It needs Studio installed and signed in. GitHub's hosted runners have neither, and a login can't be put in CI
  (secrets rule).

**B, in full.**
1. A throwaway Rust program read probe 1's JSON exports of the HUD (96 instances) and the arena (84 instances). It
   built each as a model with rbx-dom: types from the bundled reflection database, enum items by name.
2. It wrote `.rbxm` files in about 3 ms: 4.6 KB and 2.7 KB. Every property type in them was handled (booleans,
   numbers, strings, UDim, UDim2, Vector2, Vector3, Color3, CFrame, Font and enums).
3. The files went into the running Studio over localhost and were loaded with
   `SerializationService:DeserializeInstancesAsync`.
4. Each was compared with the original in the place, property by property, over every property `ReflectionService`
   lists. Values Studio computes (`AssemblyMass`, `TextBounds` and the like) were left out. Siblings with the same name
   were matched by class and position.

| | Instances | Property reads | Differences |
|---|---|---|---|
| HUD (`ServerStorage.PlayerHud_Legacy`) | 96 | 3,553 | **0** |
| Arena (`Workspace.Arena`) | 84 | 4,580 | **0** |

Loading took 56 ms (HUD) and 141 ms (arena). The bundled reflection database is from Studio 741; this Studio is 747,
and nothing in these trees was missing from it. The scripts are on the issue.

## Decision

- **B.** forge gets **`forge build`**. It turns the files on disk into a `.rbxl` or `.rbxm` with rbx-dom's crates,
  with no Studio and no login. It runs anywhere `forge` runs, CI included.
- **MeshParts** start from the `.rbxm` beside them (decision 0011). `rbx_binary` reads it, and the TOML values go on
  top, the same way a push works.
- **The reflection database can lag Studio** (741 against 747 here). forge can write a local database from the running
  Studio (`ReflectionService`, or Studio's `--fullApi`), and rbx-dom reads a local database when one is present
  (`rbx_reflection_database::get_local`). A property or enum item the database doesn't know is an error that names it,
  never a silent drop.
- **A isn't used for building.** It needs a signed-in Studio and can't save, by its docs. It stays the candidate for
  `forge test` (scripts run in a real place on the owner's PC).
- **Studio stays the source of truth for reading** (decision 0006). Building only writes, from forge's own files.

## Consequences

- A new issue, `forge build` (#50), after values and the layout (#32). It reuses that step's value forms, so it's mostly the
  writer.
- The round-trip check (#37) gains a Studio-free half: disk → `.rbxm` → Studio → compare, as this probe did.
- Of Rojo and Argon's main strengths, building without Studio is now planned. The others are already issues: a
  sourcemap (#21) and the MCP (#40).
