# 0009 Studio-only features: what a tool can reach

Accepted, 2026-11-11. Answers probe 8 (#19).

## Context

Some of Studio's most useful features live only in its windows: Script Analysis, the debugger, the Drafts window and
the Asset Manager. Agents and the CLI can't click windows. The question is which features a plugin or the MCP can
reach, so that a forge tool can offer them outside Studio.

All of this was checked on Studio 0.747 through `execute_luau`, which has plugin rights, against anime-collectors.
Everything was read-only.
- **Docs:** [DraftsService](https://create.roblox.com/docs/reference/engine/classes/DraftsService.md),
  [ScriptEditorService](https://create.roblox.com/docs/reference/engine/classes/ScriptEditorService.md),
  [Severity](https://create.roblox.com/docs/reference/engine/enums/Severity.md),
  [ScriptDebuggerService](https://create.roblox.com/docs/reference/engine/classes/ScriptDebuggerService.md),
  [ScriptProfilerService](https://create.roblox.com/docs/reference/engine/classes/ScriptProfilerService.md),
  [SceneAnalysisService](https://create.roblox.com/docs/reference/engine/classes/SceneAnalysisService.md),
  [AssetService](https://create.roblox.com/docs/reference/engine/classes/AssetService.md).
- **ReflectionService** has no data for the Studio-only services: DraftsService, ScriptEditorService,
  ScriptDebuggerService, AssetManagerService, StudioService and TeamCreateService. Each one was called directly instead.

## Results

| Feature (Studio window) | API | Reachable from a plugin? | Verdict |
|---|---|---|---|
| Drafts (commit and uncommitted changes in Team Create) | `DraftsService` | **No**: `GetDrafts`, `GetEditors` and so on refuse ("lacking capability RobloxScript") | Can't drive it. A draft can still be *detected*: a script whose `Source` differs from `GetEditorSource` |
| Script Analysis (lint and type errors) | `ScriptEditorService:RegisterScriptAnalysisCallback` | **Push only**: a plugin can add its own diagnostics to the window (registered and removed without error), but can't read Studio's | Studio's analyzer is Luau's own, and it runs on disk with `luau-lsp` once code is synced |
| Debugger | `ScriptDebuggerService` | **Yes**: `GetThreads` answered. Roblox's `rbx-debug` skill covers breakpoints, stacks and variables | A tool can offer it outside Studio |
| Script Profiler, scene and memory analysis | `ScriptProfilerService`, `SceneAnalysisService` | **Yes**: their methods are listed and plugin-callable | Already covered by Roblox's MCP skills (`rbx-perf-profiling`, `rbx-scene-analysis`); no forge tool for now |
| Asset Manager | `AssetManagerService` | **No**: the service isn't available to plugins | What it lists can be rebuilt: the place refers to 25 distinct asset ids, readable from its properties, and Open Cloud gives each one's details |

**What the Drafts window was showing.** With collaborative editing on, an edit to a script stays a local *draft*.
`Script.Source` holds the committed copy that everyone shares, and the editor shows the draft. Probe 1 found 26
scripts with an empty `Source` and more with old text. Today 73 scripts have a `Source` that differs from their editor
text. Those are drafts that haven't been committed. Roblox's docs say a draft must be committed before other
collaborators receive it. So the copy anyone else gets, and `SerializeInstancesAsync` (decision 0006), has the old or
empty text.

## Decision

- **forge sync detects uncommitted drafts and says so.** A pull or a status shows which scripts have one, and a push
  warns before writing a script that has one. Committing stays a click in Studio's Drafts window, because no API
  allows it.
- **Lint and type checks run on disk, not in Studio.** forge writes a sourcemap of the synced tree so `luau-lsp` and
  selene can resolve `require`s and instance paths, like they do for a Rojo project. Its diagnostics can be pushed back
  into Studio's Script Analysis window through `RegisterScriptAnalysisCallback`, so people see the same result.
- **The debugger gets a CLI and MCP front later.** It's a tool of its own once the plugin link (probe 5) exists.
- **Assets:** a tool that lists every asset id a place uses, with its type, name and status from Open Cloud. That
  covers what the Asset Manager shows. The window itself can't be reached.
- **Profiler and scene analysis** stay with Roblox's own MCP skills.

## Consequences

- New issues: the drafts warning in sync (#20), a sourcemap and `forge lint` (#21), a debugger tool (#22), and an
  asset inventory tool (#23).
- This changes the reason given in decision 0006, not its decision. B's stale script text comes from uncommitted
  drafts, and that's another reason forge reads the editor's text.
- If Roblox opens `DraftsService` or the Asset Manager to plugins, this record gets replaced.
