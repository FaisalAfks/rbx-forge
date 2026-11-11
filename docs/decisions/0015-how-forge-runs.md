# 0015 How forge runs: one server, a packed plugin, pushes confirmed in Studio

Accepted, 2026-11-11. Part of the build plan (#30).

## Context

Four questions came up while turning the probes into a build plan. None of them needed Studio to answer, but each is
a fork in the road.

1. Decision 0012 makes the CLI the WebSocket server and the plugin a client that reconnects. A command like
   `forge sync pull` is short-lived, though, and the plugin takes up to its retry time to find a new server.
2. The plugin has several modules (the link, each tool's Studio side, the widget). Studio loads one model file per
   plugin from `%LOCALAPPDATA%\Roblox\Plugins`, and forge doesn't use Rojo to build it.
3. A push needs a shown plan and the owner's OK (plan, AGENTS.md). An agent calling forge from the CLI or the MCP
   can't give that OK for the owner.
4. A pull writes `_.toml` files. If someone adds a comment to one, the next pull either rewrites the whole file or
   edits it in place.

## Decision

**1. One forge server per PC.**
- `forge serve` runs in the background. It holds the WebSocket the plugin connects to, on a port from config (34900
  by default).
- The plugin says hello with its place id. The server serves every game repo on the PC and matches each place to a
  repo through that repo's `forge.toml`.
- Every other command (`forge sync pull`, `diff`, `push`, `status`) and `forge mcp` is a client of the server, on the
  same port. A command starts the server when it isn't running, so nobody has to remember it.
- Against the other option, a server per command: the plugin stays connected, so a command doesn't wait for a
  reconnect, and watch, the MCP and one-off commands all share one link per Studio.

**2. The plugin is packed into one `.rbxmx` by forge itself.**
- The plugin's code is plain `.luau` files: `plugin/` for the shared parts, `tools/<name>/studio/` for each tool's
  Studio side.
- `forge plugin install` writes them into `forge.rbxmx`, a Script with ModuleScripts under it, and copies it into
  the Plugins folder. It then says Studio has to restart.
- `.rbxmx` is plain XML: Roblox's own MCP plugin, already in that folder, is one (a Script, Folders and ModuleScripts,
  each with `Name` and `Source`).
- Against one long concatenated `.lua` file: modules keep their own `require`s and names, so errors in Studio point to
  the right module, and the file layout matches the code.
- Against building the plugin with `forge sync` itself: that would need a working sync before the plugin that sync
  depends on.

**3. A push is confirmed by a click in Studio.**
- `forge sync push`, from the CLI or the MCP, works out the plan and sends it to the plugin. The plugin shows it in its
  widget, with Apply and Cancel. Nothing changes until Apply is clicked.
- The CLI waits and prints the result. The MCP's push tool returns the plan and the result, and can't apply on its
  own.
- Against a yes/no prompt in the terminal: an agent running the CLI can answer that prompt by passing `y` in, and
  nobody sees it happen. An agent could click in Studio too (the Studio MCP has mouse and keyboard tools). But
  clicking is live input on the owner's PC, which agents only do with the owner's word, and a click in Studio's
  window happens where the owner can see it. The click is a deliberate step; a `y` passed in isn't.
- How the widget looks is picked from rendered options in the push step, like any UI.

**4. Property files are written whole, in one fixed form.**
- A pull writes each `_.toml` from scratch: keys in a fixed order, values in the documented forms. Comments in property
  files aren't kept, and the format guide says so.
- The same tree always gives the same bytes, so a second pull with nothing changed is no diff at all.
- Against editing in place: that would keep comments, but the same tree could then be written two ways, depending on
  the file's history. `toml_edit` can do edits in place if a real need turns up later (0014).

## Consequences

- The build plan's link step builds `forge serve`, the plugin's link and `forge plugin install` together. Nothing can
  be pulled before they exist.
- The push step includes the plugin's widget, starting with the confirm dialog. The clash list (0013) joins the same
  widget in the watch step.
- `forge status` lists connected Studios by place, and which repo each one belongs to.
