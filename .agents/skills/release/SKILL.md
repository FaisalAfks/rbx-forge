---
name: release
description: Release a new rbx-forge version (SemVer tag and a GitHub release with notes).
---

# Release rbx-forge

1. Make sure `main` is green and everything for the release is merged.
2. On a branch `release/vX.Y.Z`, in `CHANGELOG.md`, rename `## Unreleased` to `## X.Y.Z - YYYY-MM-DD` and start a new
   empty `## Unreleased` above it. Keep its `### forge` and per-tool sections; drop any left empty. Open a PR "Release vX.Y.Z" that says `Closes #<release issue>`, and merge on green.
3. Tag `main`: `git tag vX.Y.Z && git push origin vX.Y.Z`. The release workflow builds Windows and Linux zips
   (`forge-X.Y.Z-<os>-x86_64.zip`, named for rokit) plus `forge.exe`, and makes the GitHub release with notes generated
   from the PR labels. To check the builds before tagging, run the workflow by hand: it builds without releasing.
4. Check the notes read well. Edit them in the owner's voice if needed. Never move or delete a tag: a mistake gets the
   next version.

SemVer: a fix is a patch (0.1.1), a new feature a minor (0.2.0); before 1.0 a breaking change is also a minor, noted in
the changelog.
