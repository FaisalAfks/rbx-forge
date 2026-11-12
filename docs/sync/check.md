# forge sync check

forge's own test of what Studio saves (decision 0011): does a tree survive the trip Studio -> files -> Studio? Run it in
the game's repo with the place open in Studio.

```bash
forge sync check                  # every tree in forge.toml
forge sync check StarterGui.Hud   # one tree, by its Studio path or its folder
```

For each tree it:
1. reads the tree from Studio, as pull does, and writes it to files in memory (nothing on disk changes);
2. builds a copy from those files in Studio, in a scratch folder in ServerStorage, as push builds new instances;
3. reads the copy, and compares it with the original value by value, in the written form, which is exact.

The copy is built inside an undo recording that's cancelled at the end, so the place and its undo history are as they
were, and the copy is gone.

```
ServerStorage.Hud: 96 instances, the copy matches value for value
```

Each difference is one line, in the same form as `diff.md` (original -> copy), and the command fails when there's
any. Values Studio computes and never saves (`TextBounds`, `AssemblyMass`) aren't compared, since pull never reads
them. Properties pull can't sync are listed after it, as with pull.
