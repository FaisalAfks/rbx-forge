---
name: review-pr
description: Review an rbx-forge PR against the repo's rules before it's merged.
---

# Review a PR

Check, in this order:

1. **Checks are green**, and they actually ran.
2. **One task:** the PR closes its issue and does only that; nothing unrelated slipped in.
3. **A toolbox:** nothing in a tool knows about one game; it works from the CLI and the MCP; its doc says what it
   reads, writes and never touches; scripts are left to Script Sync.
4. **Safety:** a push into Studio shows its plan and waits for an OK, and keeps a backup.
5. **Readable code:** a file summary, small functions, names that say what things are, comments that say why.
6. **Docs:** the changelog line is there; docs match the change; a decision record exists if the change sets a rule.
7. **Writing:** the PR, commits and comments are in the owner's voice, with nothing about AI or who asked.
8. **Line endings:** no whole-file rewrites (`git diff --ignore-cr-at-eol --stat` shows the real size).

Report findings as a short list, most important first, with file and line.
