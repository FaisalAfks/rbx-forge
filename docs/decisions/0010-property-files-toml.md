# 0010 Property files are TOML

Accepted, 2026-11-11. Answers probe 9 (#25). Confirms the format decision 0008 assumed without comparing.

## Context

Decision 0008 settled the layout and wrote the property files (`_.toml`) as TOML, but no other format was compared.
The candidates were TOML, JSON, JSON with comments and trailing commas (JSONC, as in VS Code's settings) and YAML.

## What was tested

The same four trees as probe 3 (HUD, arena, shared code, Ember rig) were written in layout C in each format. The
throwaway script is on the issue. Each format got the same treatment:
- one property per line;
- arrays on one line;
- keys sorted;
- leaf children as nested blocks.

Every file was then read back with a standard parser and compared with what was written: `tomllib`, `json`, JSON
with trailing commas removed, and PyYAML. The probe 3 edits were replayed, plus two more: a property added as the
last key of a block, and a label's text set to the word `no`.

| | TOML | JSON | JSONC | YAML |
|---|---|---|---|---|
| Bytes (89 files) | **45,351** | 61,416 | 61,846 | 47,648 |
| Lines | 2,175 | 2,519 | 2,519 | **2,000** |
| Values read back wrong | **0** | **0** | **0** | **5** |
| Add a button with a `UICorner` | **+8** | +12 | +12 | **+8** |
| Add a property as the last key | **+1** | +2 −1 (comma) | **+1** | **+1** |
| Move a script under a part | +14 −16 | +19 −19 | +18 −18 | **+14 −15** |
| Colour, frame move, renames | same in all four | | | |

- **YAML** read rig numbers such as `-1e-06` and `-2e-06` (in `Motor6D.C0` and attachment CFrames) back as *strings*.
  That's YAML 1.1's number rules, which PyYAML follows. Names and text like `2`, `3`, `-` and `no` also need quotes,
  or they're read as numbers, booleans or null. A writer can quote everything, but a person or an agent editing the
  file by hand gets no warning.
- **JSON** is a third bigger, and adding the last key also changes the line before it, because of the comma.
- **JSONC** fixes the comma, but it isn't one standard: trailing commas and comments need a JSONC-aware parser, and
  plain JSON tools refuse the files.

What reads and writes each format:

| | TOML | JSON | YAML |
|---|---|---|---|
| Studio (Luau) | no | yes (`HttpService`) | no |
| Lune (`@lune/serde`) | yes | yes | yes |
| Python | read (`tomllib`) | yes | needs PyYAML |
| Rust | `toml`, and `toml_edit` that keeps formatting | `serde_json` | `serde_yaml` |

Studio not reading TOML doesn't matter: the plugin sends JSON over the link and the CLI writes the files (decisions
0006 and 0008).

## Decision

- Property files stay **TOML**, as `_.toml`.
- On the wire between plugin and CLI, it's JSON.
- How each value type is written is still settled in the first build PR (decision 0008).

## Consequences

- None for the layout. Decision 0008 stands as written.
- The CLI writes TOML itself, with one property per line and sorted keys, so its output is stable and diffs stay
  small. A hand-edited file is read with a standard TOML parser and rewritten in that form on the next pull.
