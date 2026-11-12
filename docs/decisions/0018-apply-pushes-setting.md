# 0018 When a push asks for Apply is a setting in Studio

Accepted, 2026-11-12. Part of #59. Reverses part 3 of decision 0015 ("A push is confirmed by a click in Studio").

Reversed 2026-11-12, was: every push waits for Apply in Studio, with no way to turn it off.

## Context

Decision 0015 made every push wait for a click on Apply in Studio. A command, an agent or the MCP could propose a
push, but nothing changed until someone clicked where the place is open.

Working that way through a whole session is slow: every push stops for a click, even a one-line change from the
files. Rojo and Argon both let their users turn the prompt down:
- **Rojo's "Confirmation behavior"**: Initial (the default: once per project in a Studio session), Always, Large
  Changes (over a number of instances you set), and Never.
- **Argon's "Display prompts"**, with a "Changes threshold" setting. It keeps each setting per place, per game, or for
  everything.

## Decision

- **forge's plugin has a setting, "Apply pushes":**
  - *Ask every time* (the default);
  - *Ask for large pushes*, with a change count above which it asks (default 20);
  - *Never ask*.
- **The setting lives in Studio only**, stored with `plugin:SetSetting` and changed in forge's window. No command, agent,
  MCP tool or file can change it. So turning off the click is still a choice made in Studio, by whoever has the place
  open: the line decision 0015 drew is kept, it's just drawn once instead of on every push.
- **What still guards a push with no click:**
  - the plan is printed by the command;
  - a backup of everything it replaces is written to `.forge/backup/<time>/` before it applies, and `push --from` puts
    a backup back;
  - it's still one undo step, cancelled whole if anything fails;
  - forge's window logs it under Recent.
- **One setting for all places**, not per place or game as in Argon. It belongs to the person at the PC rather than to a
  game. A per-place override can come later if a game needs it.

Against keeping the click for every push: the click is the safeguard when the person running the command and the person
in Studio might differ (an agent running commands). With the setting in Studio, the person in Studio still decides
that, once.

## Consequences

- The push request carries the number of changes, so the plugin can tell a large push from a small one.
- forge's window (#59) gets settings, starting with this one and "Connect automatically".
- Docs (`docs/sync/push.md`) say what each choice does, and that the default asks.
