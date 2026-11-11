---
name: run-a-probe
description: Answer a probe issue with a test on real Studio data, and record the answer as a decision.
---

# Run a probe

1. Read the probe issue: its question, how it's tested, and what counts as done. Move it to In Progress.
2. Read the docs of what's being probed first (Roblox's docs, the library's docs and source) and note the links.
3. Look at the screen before using Studio. Work on a copy or a test place when the probe could change the game;
   never save or publish the real place without the owner's OK.
4. Run the smallest test that answers the question, on the real parts the issue names. Keep its script in the
   scratchpad; paste the script and its output into the issue, not into the repo's tool folders.
5. Measure what the issue asks (sizes, times, what's kept and what's lost), not impressions.
6. Write `docs/decisions/NNNN-<name>.md`: context, what was tested and how, the results, the decision, and what follows.
   Add it to `docs/decisions/README.md`.
7. Open a PR on `probe/<issue>-<name>` that says `Closes #<issue>`, with a `CHANGELOG.md` line. Merge on green.
