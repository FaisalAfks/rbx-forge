# 0006 Studio reads the tree with Luau (candidate A); model data only where Luau can't reach

Accepted, 2026-11-11. Answers probe 1 (#3).

## Context

The plan had two ways to get a part of the place out of Studio:
- **A:** walk the tree in Luau, list each class's saved properties with `ReflectionService`, read script text with
  `ScriptEditorService`, and build plain JSON.
- **B:** `SerializationService:SerializeInstancesAsync`, Roblox's binary model format, converted on the CLI side.

Docs used:
- [SerializationService](https://create.roblox.com/docs/reference/engine/classes/SerializationService.md): plugins
  only; services and other non-creatable instances throw; the format has no stability contract for other tools.
- [ReflectionService](https://create.roblox.com/docs/reference/engine/classes/ReflectionService.md)
- [ScriptEditorService](https://create.roblox.com/docs/reference/engine/classes/ScriptEditorService.md)
- [Instance.IsPropertyModified](https://create.roblox.com/docs/reference/engine/classes/Instance.md)

## What was tested

These ran on Studio 0.747 through the MCP's `execute_luau`, which runs inside Studio's built-in assistant plugin with
plugin rights. Nothing was parented into the place, rebuilt copies were destroyed, and the place wasn't saved. The
targets were:
- the old HUD (`ServerStorage.PlayerHud_Legacy`, 96 instances);
- the arena (84);
- `ReplicatedStorage.Shared` (68, 56 of them scripts);
- the Camera module, which has 3 attributes;
- a tagged Sound.

Each target was read 5 times and the median taken. A's JSON was then rebuilt into new instances, and every readable
property of every instance was compared with the original. The scripts and full output are on issue #3.

A writes a property only when:
- it's saved (`Serialized`), readable and writable, and not deprecated;
- and `IsPropertyModified` says it differs from the default.

## Results

**Speed and size** (median of 5)

| Target | A read | A rebuild | A JSON | B serialize | B deserialize | B rbxm |
|---|---|---|---|---|---|---|
| HUD | 2.5 ms | 0.9 ms | 20 KB | 11.9 ms | 1.0 ms | 15 KB |
| Arena | 1.6 ms | 1.0 ms | 23 KB | 13.4 ms | 0.6 ms | 6 KB |
| Shared (code) | 1.9 ms | 4.9 ms | 298 KB | 648 ms | 0.9 ms | 124 KB |
| Camera module | 0.4 ms | 0.6 ms | 33 KB | 185 ms | 0.3 ms | 1 KB |

- B's rbxm is compressed binary. A's JSON includes script text, which becomes `.luau` files on disk (probe 3), so the
  byte counts aren't a like-for-like comparison.
- B's serialize time varies a lot from run to run.
- Two of the first `SerializeInstancesAsync` calls never answered, and the MCP timed out. The same calls worked
  afterwards. The cause is unexplained. Any use of B needs a timeout guard.

**B loses script text.** B saves `Script.Source`, and in this place that property is stale or empty. The editor text
from `ScriptEditorService` matches the repo's files byte for byte (for example `Shared.Net.Remotes` is 5,249 bytes on
disk and in the editor, while `.Source` holds 4,707). B's copy didn't match the editor text for:

| Folder | Scripts where B's copy is wrong | Of these, `.Source` empty |
|---|---|---|
| `ReplicatedStorage.Modules` | 23 of 23 | 23 |
| `Shared` | 19 of 56 | 3 |
| `ServerScriptService.Server` | 13 of 31 | 2 |

The rbxm's 1 KB for the Camera module is a 14 KB script exported as empty.

**A's round trip is exact for everything saved.**
- Across all five targets, 0 saved properties differed, and so did 0 script texts, attributes and tags.
- What did differ is values Studio computes and never saves (no write permit, not serialized): `TextBounds`,
  `TextFits`, `AssemblyMass`, `AssemblyRootPart`, and `Sound.IsLoaded` and `TimeLength`. They differ because the
  copies weren't in the place.
- A first version wrote every readable and writable property. It got colours wrong, because `BrickColor` and `Color`
  are two views of one value and the order they're set in decides the result. Writing only saved properties fixed it.

**References**
- None of the real targets has a reference inside it.
- On a synthetic copy of the arena (two parts sharing one name, an `ObjectValue` and a `PrimaryPart`), references
  written as an index into the walk all landed on the right instance. Written as full names, one landed on the wrong
  instance.
- Paths can't identify instances. That's input for probes 2 and 3.

**What Luau can't read.** Some saved properties have no read permit. Across the 203 classes in the place they fall
into three groups:
1. **A readable twin under another name:**
   - `BasePart.Color3uint8`→`Color`, `size`→`Size`, `MaterialVariantSerialized`→`MaterialVariant`;
   - `Part.shape`→`Shape`;
   - `Model.WorldPivotData`→`WorldPivot`;
   - `Humanoid.Health_XML`→`Health`;
   - and, found on a `WeldConstraint` the place doesn't have, `Part0Internal`/`Part1Internal`→`Part0`/`Part1`.
2. **Reachable only through methods:**
   - `Model.ScaleFactor` (`GetScale`/`ScaleTo`);
   - `StyleRule` properties (its property methods);
   - `LocalizationTable.Contents` (its entry methods);
   - Terrain voxels (voxel reads).
   
   B does serialize Terrain.
3. **Not reachable from Luau at all:**
   - `LuaSourceContainer.ScriptGuid`, `Model.ModelMeshData`, `WeldConstraint.CFrame0`/`State`;
   - about 40 Workspace, StarterPlayer, StarterGui, Lighting and SoundService behaviour flags.
   
   B refuses Workspace and Lighting outright, since services can't be serialized, so only the place file carries
   these flags.

**Not tested:** a separately installed plugin. `execute_luau` runs with plugin rights inside Roblox's own plugin.
Running from forge's own plugin belongs to probe 5 (the link), which needs that plugin anyway.

## Decision

- **Studio reads and writes the tree with Luau (A).** Studio builds the readable form itself. The CLI never parses
  Roblox's binary format for the main path.
- **Saved properties only**, plus a **twin list**: which hidden saved property is held by which script-facing one.
  Method-only values get per-class handlers. Both lists live in forge's config, not in the walk's logic.
- **Nothing drops silently.** On start, forge compares the running Studio's reflection data with the twin and
  handler lists. Any saved, unreadable property with no entry is reported for the classes being synced.
- **References are written by forge's own identity, never by path.** How that identity looks is probe 2's question.
- **B is a fallback only**, as an opaque `.rbxm` beside the readable files, for an instance whose class has
  saved data that A can't reach (Terrain is the first case), and always with a timeout. It's never used for scripts.
- Script text stays with `ScriptEditorService` only (decision 0005). Here that's a hard rule: `.Source` was wrong for
  55 of the 110 scripts checked in those three folders.

## Consequences

- Probe 2 drops its "do Lune and rbx-dom read B's data" half. That matters only for the fallback blobs, which forge
  stores but doesn't parse. Identity becomes its main question.
- Probe 3's layouts have to place references and the occasional `.rbxm` blob, not only properties and scripts.
- Place-level flags with no Luau access stay in the place file. forge says so in its docs ("what it never touches").
- The twin list will grow as games use more classes. The check at start is what keeps it honest.
