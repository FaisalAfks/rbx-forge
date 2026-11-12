# 0022 A plain Folder needs no `_.toml`

Accepted, 2026-11-12. Part of #67. Work in #77. Reverses part of decision 0008 ("The folder's `_.toml`").

Reversed 2026-11-12, was: every instance with children is a folder holding its own `_.toml`.

## Context

A game's code is mostly Folders of scripts. In layout C each of those Folders has a `_.toml` that says only
`class = "Folder"`. anime-collectors' code would get 35 of them: files nobody edits, that every new folder needs, and
that Luau tools and Rojo don't have. Decision 0020 already lets a script's folder go without one.

Two options were compared:
- **Keep a `_.toml` in every folder.** One rule with no exceptions, at the cost of a file per folder that says nothing.
- **A folder with no `_.toml` is a Folder** (chosen). It reads the way Luau tools and Rojo read a folder, and the code
  folders hold only code.

## Decision

- A folder with no `_.toml` and no init file is a Folder, named by its disk name.
- Pull writes a Folder's `_.toml` only when there's more to say: a name that was escaped, tags, properties, attributes,
  or children without children of their own, which are sections in it.
- The tree's top folder always has its `_.toml`: it holds the tree's name and marks the folder as forge's.

## Consequences

- Trees pulled before still read the same. The next pull removes the `_.toml` files that said only `class = "Folder"`.
- A lost `_.toml` in a folder of another class reads as a Folder, and diff shows it as a class change rather than an
  error.
