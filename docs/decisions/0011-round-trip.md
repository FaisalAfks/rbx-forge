# 0011 Round trip: exact, with MeshParts starting from a small model file

Accepted, 2026-11-11. Answers probe 4 (#6).

## Context

Probe 4 asks whether Studio → disk → Studio gives back the same objects, property by property. Its issue was written
before decision 0006 and planned to turn files back into model data. Under 0006, Studio rebuilds instances itself
from the readable files, so that's what was tested.

## What was tested

Four trees went Studio → disk → Studio, 347 instances in all:
- the old HUD;
- the arena;
- `ReplicatedStorage.Shared`, with 56 scripts;
- the Ember rig, with 14 MeshParts and 31 references.

The steps:
1. **Studio → disk.** The probe 3 export was written in layout C as TOML (decisions 0008 and 0010).
2. **Disk → tree.** A throwaway reader turned the folders, `_.toml` files and `.luau` files back into one tree per
   root, with values left as they are on disk. It found 96, 84, 68 and 99 instances, all of them.
3. **Tree → Studio.** Studio fetched each tree from a local server and rebuilt it unparented:
   - values were decoded by each property's type from `ReflectionService`;
   - `"@path"` references were resolved after every instance existed;
   - script text went in through `UpdateSourceAsync`.
4. **Compare.** Every readable property of every original instance was compared with its rebuilt copy, matched by
   disk path, along with attributes, tags, script text and references (by target path).

Nothing was parented into the place, and the place wasn't saved. Scripts and output are on the issue.

## Results

| Tree | Instances | Properties compared | Differences |
|---|---|---|---|
| HUD | 96, none missing | 3,595 | only `TextBounds`, `TextFits` (computed by layout) |
| Arena | 84, none missing | 4,870 | only `AssemblyMass`, `AssemblyRootPart` (computed by physics) |
| Shared | 68, none missing | 556, plus 56 script texts | none |
| Rig | 99, none missing | 2,844, plus 31 references | `MeshPart` mesh and mass (below), and `Attachment.WorldRotation` |

**MeshParts can't be rebuilt from properties.**
- `MeshId` and `RenderFidelity` are saved but have no write permit, so the plain rebuild had no mesh.
- `AssetService:CreateMeshPartAsync` with `MeshPart:ApplyMesh` restores the mesh: `MeshId`, `MeshSize` and
  `RenderFidelity` matched. It takes about 350 ms per distinct mesh, 4.9 s for the rig's 14.
- Even then, **mass came back 8% to 16% lower** (`LeftLowerArm` 0.736 → 0.620). The part's own collision data
  isn't readable, and `ReflectionService` doesn't list it at all. A `SerializeInstancesAsync` copy keeps mass on 14
  of 14.
- **What works:** each MeshPart starts from a small `.rbxm` of the part alone, without its children, and the TOML
  values and readable children are applied on top. Then every saved property, mass included, matched. The 14 rig
  files total 48,888 bytes, about 3.5 KB each. The place has 65 MeshParts.

**`Attachment.WorldRotation`** differed on all 48 attachments by an equivalent Euler form, such as
`(-180, -45.08, -180)` against `(0, -134.92, 0)`. That's the same rotation. It showed up because the disk files
rounded floats to 6 decimals, which nudged CFrames by less than 1e-6. An unparented clone with full precision showed
0 of 48.

## Decision

- **The round trip works** for GUI, map parts, scripts, attributes, tags and references. Build on it.
- **Floats are written at full precision:** the shortest text that reads back as the same number, never rounded.
- **MeshParts:**
  - each MeshPart is a leaf section, or a folder, like any instance, plus `<name>.rbxm` beside it, holding the part
    alone;
  - on a push, the part starts from that file, and the TOML values and children go on top;
  - so a colour or size change is still a readable one-line diff, and only a new mesh or new collision data changes
    the binary file.
  
  Decision 0006 already allowed this fallback ("only where Luau can't reach").
- **Attributes of non-primitive types** (Vector3, Color3, CFrame and so on) are written as `{ type = "...", value =
  ... }`. TOML can't tell them apart from arrays or strings otherwise. Strings, numbers and booleans stay plain.
- **The comparison used here becomes forge's own check.** `ReflectionService` doesn't list everything Studio saves
  (MeshPart collision data, for one), so the start-up check from 0006 can't catch every loss on its own. A round trip
  on real Studio data is the test that can.

## Consequences

- The twin and handler list from 0006 gets its first "starts from a model file" class: `MeshPart`. Other
  asset-backed classes (`UnionOperation`, `SurfaceAppearance`) aren't in this place. They're checked the same way when
  a game has them.
- 65 MeshParts in anime-collectors means about 230 KB of binary files. Git handles that; LFS isn't needed at this
  size.
- The round-trip check needs Studio, so it runs as a probe-style test through the MCP, not in CI.
