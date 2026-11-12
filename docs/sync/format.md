# How forge sync writes a tree on disk

The reference for layout C (decision 0008). Code: `tools/sync/src/layout.rs`, `names.rs` and `value.rs`. Every file
is written whole, in one fixed form (decision 0015), so the same tree always gives the same bytes and an unchanged
pull changes no file. Comments in these files aren't kept.

## Folders and files

Each synced tree (a `[[sync]]` in `forge.toml`) is a folder.

| What | Where |
|---|---|
| An instance with children | a folder, holding its own `_.toml` |
| An instance with no children | a section `[Name]` in its parent's `_.toml` |
| A script's text | `Name.luau` in its parent's folder, byte for byte (CRLF, a BOM, a missing last newline all kept) |
| A MeshPart's model (decision 0011) | `Name.rbxm` in its parent's folder |
| The tree's top instance | the folder itself; its text and model, if any, are `_.luau` and `_.rbxm` inside it |

A `_.toml` holds, in this order:
1. `class = "Frame"`.
2. `name = "..."`, only when the disk name isn't the instance's real name. The tree's top `_.toml` always has it.
3. `tags = [...]`, sorted, when there are any.
4. Properties, by name. Only saved properties that differ from their default are there (decision 0006).
5. `[attributes]`, when there are any.
6. One section per child without children, by disk name: `[Name]`, then that child's own `class`, `name`,
   `tags`, properties and `[Name.attributes]`.

A table with a `class` key is a child. No table-shaped value (Font, PhysicalProperties, Ray) has one, so this holds
even after a Studio update adds a property with the same name as an existing child. A child that gets children of
its own moves from its parent's `_.toml` into a folder.

Sibling order isn't stored. Neither folders nor TOML sections keep it, and UI order comes from `LayoutOrder`.

Files and folders starting with `.` are skipped. Any other file forge didn't write is an error that names it.

Pull also writes a `.gitattributes` with `* -text` into each synced folder, so git never changes the bytes above
(`pull.md`).

## Names on disk

An instance's disk name is its name, made safe:
- These characters become `%XX` (their code in hex): `< > : " / \ | ? * % ~`, and control characters. So do:
  - a trailing space or dot (Windows drops them);
  - the dot of a name ending in `.luau`, `.rbxm` or `.toml`, so it can't clash with forge's own files.
- These get their first character escaped (`CON` becomes `%43ON`):
  - Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM0`-`COM9`, `LPT0`-`LPT9`, with or without an extension);
  - `_`;
  - `class`, `name`, `tags` and `attributes`;
  - any name that's a property of the parent's class. A TextButton's child named `Text` is `%54ext`, so it can't
    clash with the button's own `Text = "..."`.
- An empty name is `~0`.
- An escaped name over 100 characters is cut to 80, plus `~h` and a hash of the full name.
- Siblings are unique **ignoring case**, since Windows and macOS file systems ignore it. A repeat gets `~2`, `~3`
  and so on.
  - Which repeat gets which number follows their content, so the same tree always gets the same names.
  - Names read from disk are kept when they still fit. Changing one of two `Strip`s changes one line, not a rename.

The same disk name is used everywhere: the section, the folder, `.luau` and `.rbxm` files, and references.

## Values

The types come from the schema (class, property and type, from `ReflectionService`), because the files don't say
them. `[0, 340, 0, 66]` could be a UDim2 or a Rect.

| Type | Written as | Example |
|---|---|---|
| bool | `true` / `false` | `Visible = false` |
| int, int64 | a whole number | `LayoutOrder = 2` |
| float, double | the shortest text that reads back as the same number | `BackgroundTransparency = 0.30000000000000004` |
| string, ContentId | a string | `Text = "Play"` |
| Content | its URI, as a string | `Image = "rbxassetid://1"` |
| Enum | the item's name | `TextXAlignment = "Left"` |
| BrickColor | its number | `BrickColor = 194` |
| Color3 | `"#rrggbb"` when exactly 8-bit, else `[r, g, b]` | `BackgroundColor3 = "#121520"` |
| ColorSequence | `[[time, colour], ...]` | `Color = [[0, "#ff0000"], [1, [0.2, 0.4, 0.6]]]` |
| NumberSequence | `[[time, value], ...]`, with a third number when the envelope isn't 0 | `[[0, 0], [1, 1, 0.5]]` |
| NumberRange | `[min, max]` | `Lifetime = [1, 2.5]` |
| CFrame (and optional CFrames) | 12 numbers: position, then the rotation row by row | `CFrame = [35, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1]` |
| Faces, Axes | a list of names | `Faces = ["Top", "Left"]` |
| SecurityCapabilities | a list of capability names | `Capabilities = ["RunServerScript"]` |
| Font | `{ family, weight, style }` | `{ family = "rbxasset://fonts/families/Arial.json", weight = "Bold", style = "Normal" }` |
| PhysicalProperties | `{ density, friction, elasticity, friction_weight, elasticity_weight, acoustic_absorption }` (the last one optional) | |
| Ray | `{ origin = [x, y, z], direction = [x, y, z] }` | |
| Rect | `[min x, min y, max x, max y]` | `SliceCenter = [1, 2, 3, 4]` |
| UDim | `[scale, offset]` | `CornerRadius = [0, 8]` |
| UDim2 | `[x scale, x offset, y scale, y offset]` | `Size = [1, 0, 0, 66]` |
| Vector2, Vector3, Vector3int16 | `[x, y]`, `[x, y, z]` | `AnchorPoint = [0.5, 1]` |
| Ref | a path, see below | `PrimaryPart = "@World/Strip"` |

**Numbers** keep every bit (decisions 0011 and 0014):
- A whole float up to 2^53 is written without a decimal point (`1`, not `1.0`). Above that it keeps `.0` or an
  exponent, so it never reads as an integer too big for TOML.
- Very small or large numbers use an exponent (`1e-7`, `5e-324`).
- `-0.0`, `inf`, `-inf` and `nan` are written as such. The shortest text comes from Rust's standard float
  formatting, which is exact the same way the `ryu` algorithm named in decision 0014 is.

**Colours:** Studio stores each part as an `f32`. `#rrggbb` is used only when each part is exactly `k / 255` as Studio
stores it (checked against `Color3.fromRGB` for all 256 values). HDR colours and other values are lists.

**Attributes** have no schema, so:
- strings, booleans and numbers are plain;
- every other type is `{ type = "Vector3", value = [1, 2, 3] }` (decision 0011);
- attribute numbers are always floats.

**Not in these files:** Luau can't read four engine types, so they're never written to TOML: BinaryString,
SharedString, NetAssetRef and Color3uint8. Twins (Color3uint8 is `Color`) or a MeshPart's `.rbxm` carry them.

## References

| Points at | Written as |
|---|---|
| the tree's top instance | `"@"` |
| an instance in the same tree | `"@"` and disk names from the top: `"@World/Strip~2"` |
| an instance outside the tree | `"@/"` and the names from `game` down, with only `%` and `/` escaped: `"@/Workspace/Arena"` |

A reference to an instance that isn't in the tree is an error when writing, never left out. A path that points at
nothing is an error when reading. Renaming an instance rewrites the references to it in the same change.

## Errors

Reading stops at the first problem and names the file, the key and the expected form. For example:
`Bar/_.toml: Size: expected [x scale, x offset, y scale, y offset]`.

Problems it catches:
- unknown properties and wrong shapes;
- a missing `_.toml`;
- a `.luau` or `.rbxm` file with no instance to belong to;
- stray files;
- the same name as both a section and a folder;
- a class the schema doesn't know.
