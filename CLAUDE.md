**Policy — do not merge a PR you authored.** Opening PRs is fine; merging is the operator's call. When you believe a PR is ready, say so and hand the operator a copy-pasteable command to run themselves rather than merging it:

`! gh pr merge <N> --squash --delete-branch`

(the leading `!` runs it in-session). Never run the merge yourself. `.claude/settings.json` also denies `gh pr merge` here as a backstop.

@README.md
