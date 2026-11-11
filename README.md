# rbx-forge

Roblox tooling. **Build tools, not just games.**

Every tool has a CLI and an MCP, so agents, scripts and people all use the same thing, and a Studio plugin side where
it needs one. A tool never knows about one game: a game says what it wants in its own `forge.toml`.

## Tools

- **sync** (planned): a game's UI, maps and models in git as readable files, and back into Studio. Code stays with
  Studio's Script Sync.

What's built next is in [docs/plan.md](docs/plan.md); why, in [docs/decisions/](docs/decisions/README.md). How to work
in the repo is in [.agents/AGENTS.md](.agents/AGENTS.md).
