# 0020 Script files are named the way Luau tools read them

Accepted, 2026-11-12. Part of #70. Work in #72. Reverses part of decision 0008 ("Scripts" and "There's no class
suffix in file names").

Reversed 2026-11-12, was: a script's text is always `<name>.luau` in its parent's folder, its class sits in TOML (its
parent's `_.toml`, or its own folder's), and file names carry no class suffix.

## Context

Decision 0008 chose layout C on its own merits and kept script files plain: `<name>.luau`, with the class in TOML.
That works for forge, but nothing else in the Luau world reads it:
- luau-lsp, selene and Lune resolve a folder as a module through its `init.luau`, and string requires
  (`require("./Combat")`) do the same;
- a sourcemap needs each file's class, which layout C only has inside TOML;
- every Rojo, Argon and Wally project, and every package, uses `init.luau` and the `.server` / `.client` suffixes.

So a game's code could move into forge (#67) only by losing lint, type checks and the headless tests that load
modules from disk.

Three options were compared:
- **Keep layout C's script names.** forge stays self-consistent, but every outside tool needs forge's sourcemap or
  a translation step, and Lune can't run tests from the files at all.
- **Copy Rojo whole:** project files, `.meta.json`, `.model.json`. The names would match, but forge would lose its
  TOML property files (decision 0010) and the two-way sync built on them, which is why forge exists.
- **Take only the file names** (chosen). The names are the shared convention of the Luau tools, not Rojo's alone;
  everything else in layout C stays.

## Decision

- **Script names:**
  - `Name.luau` is a ModuleScript;
  - `Name.server.luau` is a Script;
  - `Name.client.luau` is a LocalScript.
- **A script with children** is a folder `Name/` holding `init.luau`, `init.server.luau` or `init.client.luau`, plus
  its children as usual.
- **TOML only when needed:** a script gets a section (leaf) or a `_.toml` (folder) only for properties, attributes
  or tags that aren't the defaults. A plain module is just its `.luau` file.
- **Names that would read as something else are escaped:** an instance whose name ends in `.server` or `.client`, or
  is `init`, gets its dot or name escaped with `%XX`, as other reserved characters are.
- **Everything else in layout C is unchanged:** folders only where there are children, `_.toml` for other classes,
  `~2` for repeated names, `@path` references, `forge.toml` as the only project file.

## Consequences

- Pull, diff, push, check and watch read and write the new names (#72). A game synced with the old names moves over
  with one pull, which rewrites the files; the diff is renames.
- A pulled tree passes luau-lsp and selene with a sourcemap built straight from file names (#21).
- Wally and pesde packages already use these names, which makes #66 a small reader instead of a translator.
