# 0016 Seeing UI: forge stages a screen, captures it itself, and crops it

Accepted, 2026-11-11. Answers probe #45.

## Context

Agents change UI best when they can look at the result, change it, and look again. The Studio MCP's
`screen_capture` returns an image of Studio's viewport. The question was whether that's enough, or whether forge needs
a way of its own to show an agent any screen of a game, at several sizes.

## What was tested

Everything ran in Studio's edit mode through `execute_luau` and `screen_capture`, on anime-collectors, with a
990 × 568 viewport at 1.5× display scaling (1485 × 852 pixels). Every test instance was removed afterwards; the place
has nothing new in it. The scripts are on the issue.

| Test | Result |
|---|---|
| A ScreenGui in StarterGui, captured with the MCP's `screen_capture` | **Shown**, over the 3D view, at full pixel size |
| A ScreenGui kept outside StarterGui (`ServerStorage.PlayerHud_Legacy`) | Not shown where it is. A copy put in StarterGui was shown in full |
| A phone screen (844 × 390): the HUD's children moved into a frame of that size, with a `UIScale` to fit the viewport and a grey backdrop | **Laid out as on a phone**, with the 3D view hidden behind the backdrop |
| A capture taken by a plugin: `CaptureService:CaptureScreenshot`, then `AssetService:CreateEditableImageAsync` on the result | **Works in edit mode.** It returns an `rbxtemp://` image in 0.6 s, and its pixels read as a 1485 × 852 RGBA buffer (5 MB) in 3 ms. A pixel in the HUD's health bar read as its green |
| That buffer through `HttpService:JSONEncode` | 2.4 MB of text in 16 ms, under the WebSocket's tested 16 MB (decision 0012) |
| `StudioDeviceEmulatorService` (Studio's device emulator) | Not usable: `GetService` gave nothing usable to a plugin |
| UI built by scripts while the game runs, captured during a playtest | **Not tested.** Starting a playtest is live input, and the screen couldn't be checked first (access to Studio's window was refused). The MCP's `screen_capture` says it captures edit time only |

## Decision

forge gets a UI tool, **`forge ui shot`**, built on the link (decision 0012), not on the MCP's `screen_capture`:
1. **Stage.** The plugin copies the chosen ScreenGui into StarterGui inside a stage: a backdrop, and a frame of the
   chosen screen size with a `UIScale` that fits it into the viewport. The original is never touched.
2. **Capture.** The plugin takes the screenshot itself (`CaptureService`), reads its pixels (`EditableImage`), crops
   them to the stage's frame, and sends them to the CLI.
3. **Write.** The CLI writes a PNG and prints its path, so any agent that can read an image file can see it, with or
   without the Studio MCP.
4. **Clean up.** The stage is removed, inside a `ChangeHistoryService` recording that's cancelled, so no undo step is
   left behind.

Why this, and not just the MCP's `screen_capture`:
- **forge's capture comes back as a file** an agent can keep, compare and diff. The MCP's image only lives in one
  agent's conversation.
- **The crop removes everything that isn't the screen** (backdrop, other UI, Studio's selection boxes), so what's
  judged is the screen alone.
- **Sizes come from forge's settings** (phone, tablet, desktop and any other), not from the viewport the owner
  happens to have.
- **The MCP's `screen_capture` stays useful** for a quick look at the whole viewport, the 3D world included.

## Limits

- **The capture's real resolution is the viewport's**, so a large screen (4K) is captured shrunk. Detail at that size
  needs the viewport made bigger, or a capture in parts. Neither is planned until it's needed.
- **This emulates a screen size, not a device.** The safe area (a phone's notch), touch controls and Roblox's own top
  bar aren't drawn. A UI that reads the screen size in code (`AbsoluteSize`, `ViewportSize`) sees Studio's real size,
  not the stage's.
- **UI built at run time isn't covered yet.** It needs a playtest, which is live input. That gets its own probe, run
  only with the owner's word.

## Consequences

- A new issue: `forge ui shot` (#47), after the link step (#33). Device sizes go in `forge/defaults.toml`.
- Capturing during a playtest is its own probe (#48).
- The plugin's capture is reusable: `forge sync check` (#37) or a future visual diff can compare two captures of the
  same screen.
