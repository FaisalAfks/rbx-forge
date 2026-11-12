# forge sync diff

Lists what differs between Studio and a game's files, tree by tree, going from disk to Studio. Run it in the game's
repo with the place open in Studio. It reads Studio the same way pull does (`pull.md`) and changes nothing anywhere.

```
ServerStorage.Hud -> game/hud: 3 differences, disk -> Studio
  ~ Root/Bars/Health BackgroundColor3: "#1c2130" -> "#ff0000"
  > Root/Portrait/LevelPill -> Root/LevelPill (Frame): moved or renamed
  ~ Main script: +1 -1 lines
```

Paths are disk paths from the tree's folder (`(top)` is the tree itself). Values are shown as they're written on disk,
with the disk side first.

| Line | Means |
|---|---|
| `+ path (Class, N inside): in Studio only` | an instance, and N below it, that the files don't have |
| `- path (Class, N inside): on disk only` | an instance, and N below it, that Studio doesn't have |
| `> from -> to (Class): moved or renamed` | the same subtree under another path |
| `~ path Property: disk -> Studio` | a property, an attribute (`attribute Name`) or the tags; `(unset)` when one side has none |
| `~ path script: +A -R lines` | a script's text, with lines added and removed |
| `~ path model file: ...` | a MeshPart's mesh or collision data |
| `! path: class X on disk, Y in Studio` | the same path holds another class |

A subtree found whole under another path is one move, not a removal and an addition. Without watch's live map, a
move is found by content, so a subtree that was moved and edited at once shows as removed and added.

A tree that was never pulled says `not pulled yet`. When nothing differs it says `disk and Studio match`. Scripts with
an uncommitted draft are listed after it, as with pull.
