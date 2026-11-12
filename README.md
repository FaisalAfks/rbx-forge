# rbx-forge

Roblox tooling. **Build tools, not just games.**

Every tool has a CLI and an MCP, so agents, scripts and people all use the same thing, and a Studio plugin side where
it needs one. A tool never knows about one game: a game says what it wants in its own `forge.toml`.

## Tools

- **sync**: a whole game in git as readable files, code, UI, maps and models as one nested tree, and back into
  Studio ([guide](docs/sync/README.md)).

How the repo holds tools, and adding one: [docs/tools.md](docs/tools.md).

What's built next is in [docs/plan.md](docs/plan.md); why, in [docs/decisions/](docs/decisions/README.md). How to work
in the repo is in [.agents/AGENTS.md](.agents/AGENTS.md).
