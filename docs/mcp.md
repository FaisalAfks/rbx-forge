# forge's MCP server

`forge mcp` serves every forge tool's MCP tools (each tool's `mcp_tools`, docs/tools.md) to an MCP client (an agent)
over stdio (`mcp/src/lib.rs`). Each tool runs the
same `forge` command a person would, in the game's repo, and returns what it printed, so the two can't drift apart. A
command that fails comes back as an error with its own message.

## Adding it to a client

Most clients take a command and its arguments:

```json
{ "mcpServers": { "forge": { "command": "forge", "args": ["mcp"] } } }
```

Start the client in the game's repo, or give each tool `repo`: the game's folder (the one with `forge.toml`, or below it).

## Tools

| Tool | Runs | Changes |
|---|---|---|
| `status` | `forge status` and `forge sync status` | nothing |
| `pull` (`force`) | `forge sync pull` | the game's files |
| `diff` | `forge sync diff` | nothing |
| `push` | `forge sync push` | Studio, **only after Apply** (below) |
| `check` (`tree`) | `forge sync check` | nothing (its copy is taken away) |
| `clashes` | `forge sync status` | nothing |
| `pick` (`number`, `side`) | `forge sync pick` | a clash a running watch holds |

`watch` isn't a tool: it runs until stopped, so it belongs in a terminal.

## Pushes stay the person's call

A push from the MCP is proposed like any other. It applies only when Apply is clicked in forge's window in Studio,
unless the person in Studio set forge's "Apply pushes" setting not to ask (decisions 0015 and 0018). No tool can
change that setting. The tool waits for the click, and its result says whether the push was applied, cancelled or left
to time out: an agent reads that result, never assumes.
