**Policy — do not merge a PR you authored.** Opening PRs is fine; merging is the operator's call. When you believe a PR is ready, say so and hand the operator a copy-pasteable command to run themselves rather than merging it:

`! gh pr merge <N> --squash --delete-branch`

(the leading `!` runs it in-session). Never run the merge yourself. `.claude/settings.json` also denies `gh pr merge` here as a backstop.

**Work order — the roadmap is the GitHub project, not an issue.** The roadmap is [sw2m project 1](https://github.com/orgs/sw2m/projects/1/views/1) with its milestones (`v1.0.0` first); the old roadmap issue #10 is retired. Take tickets in ascending **Start date**, within the current milestone. Read the order live (`gh project item-list 1 --owner sw2m`, which needs the `project` token scope); never copy it into a file or issue, where it goes stale.

@README.md
