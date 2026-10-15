# After the push

`make ship` pushes a reviewed commit and arms auto-merge. While the pull request
waits for CI, `dev` may move, and GitHub would merge the pull request untested
against the new `dev` (four incidents in two days, #388). Three pieces close
that window; the workflow `.github/workflows/post-push.yml` only decides when
they run, the logic is in `crates/harness/src/ship/postpush/`.

**The setting is on.** "Require branches to be up to date before merging" is
on for `dev` (`strict: true`) since 2026-10-15, after the updater, the carry and
the dispatched `required` status were seen working on #406: GitHub reported the
dispatched run's `required` status as the required check. A pull request
therefore merges only when its CI passed on top of the current `dev`. `dev`
moves only when a pull request merges, and each merge restarts the waiting pull
requests' CI, so at most about one waiting pull request merges per CI run
(about 20 minutes). The code changes no repository setting, branch protection
or ruleset.

## 1. The marker

When `make ship` (or `make review-pr`) posts `harness/review` into `dev`, it
also writes a pull request comment that starts with the hidden line
`<!-- aoe-harness:review-marker -->`, then one JSON comment line: the approved
`head`, its own-patch `patch` identity (SHA-256 of the exact changed bytes
against the base, as `review/reuse.rs` computes it, recomputed from Git), the
review `policy` fingerprint (`Report::policy_fingerprint_at`), `tier` and
`grade`. The next passing review edits that comment in place; if it cannot,
it adds one. A comment, not the status description: a description holds 140 characters and
the two fingerprints are 128. A marker that cannot be written is a warning;
the ship still succeeds, only a later update will not carry the review.

Only a marker from `OWNER`, `MEMBER` or `COLLABORATOR` counts, the newest one:
anyone can comment on a pull request, and a forged marker would let a changed
patch through.

## 2. Carrying `harness/review` (`make carry-review`)

On `pull_request_target` `synchronize` into `dev`, with `CARRY_PR=<number>`. The
event is `pull_request_target` so that the workflow and the harness come from
`dev`: the pull request's head is fetched as Git data (`refs/pull/N/head`) and
none of its code runs. The command computes the head's own-patch identity against
the current `dev` and the review policy fingerprint at `dev`, with the marker's
tier. If both equal the marker's, it posts `harness/review` = success on the
new head, "carried from <sha12>: patch and policy unchanged". Otherwise it posts
nothing and writes one comment saying which: `patch changed`, `policy changed`
or `no review marker`. A successful carry also comments on the updated head, so
`make next` can show open PRs into dev with auto-merge armed whose latest carry
comment on their current head is a refusal, first, as `reship: <reason>`.
Comments on older heads do not count. It never posts success for a head whose own patch
differs. This is the rule of the local re-check carry (#367) and of the
unchanged-patch reuse (#386), run where the update happens. Token: `statuses:
write` and `pull-requests: write`.

## 3. Updating waiting pull requests (`make update-prs`)

On every push to `dev`: each open pull request into `dev` with auto-merge armed
and behind `dev` (GitHub's compare: `behind_by > 0`) gets `dev` merged in with
the GraphQL `updatePullRequestBranch`, method MERGE, pinned to the head the run
saw. Not REBASE: GitHub rewrites rebased commits unsigned (committer
`github-actions[bot]`), and `dev` requires signed commits on the head branch,
even for a squash merge (seen on #406 and #417, 2026-10-15). The squash merge
keeps `dev` linear. At most `UPDATE_MAX_PRS` (5) pull requests per run,
oldest first; one run at a time (concurrency group `post-push-update`, never
cancelled halfway). An update that GitHub refuses with conflicting files leaves
the pull request untouched and adds one comment naming the files (once per
head). Conflict diagnosis replays the PR commits in order in a temporary detached
worktree, naming the files of the first conflicting commit, even if a later
commit restores their original contents. The worktree is removed afterwards.
A refusal with no conflict also adds one comment explaining the API failure
and any local replay failure, and remains an error of the run. Forks are skipped.

The updater uses the workflow's own `GITHUB_TOKEN`, with `contents: write`,
`pull-requests: write`, `statuses: write` and `actions: write`; no personal
secret is needed. A push made with this token starts neither `pull_request` CI
nor the `pull_request_target` carry job. After GitHub accepts an update, the
updater polls the PR's head every five seconds for up to two minutes until it
changes, calls the existing review carry, then dispatches `ci.yml` on the PR's
head branch. `workflow_dispatch` is an exception to GitHub's token recursion
rule: a dispatch made with `GITHUB_TOKEN` does start a workflow run. The
harness chooses the merge base of the head with `origin/dev` as the comparison
base for dispatches on feature branches, with the `pr` cadence, just like a pull
request; the merge base does not change when dev advances between the selection
and the final check. The `required` status step does not run on `dev`/`main`.
The dispatcher supplies no base. Dispatches on `dev`/`main` and other events
without a base still select all gates; an unavailable comparison also selects all.

[GitHub does not evaluate dispatch-run workflow-job checks for pull requests](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#checks-from-some-workflow-jobs-are-not-evaluated).
The `required` job therefore finishes every dispatch with `make ci-dispatch-status`:
it posts a commit status named `required` on `GITHUB_SHA`, using the workflow's
`GITHUB_TOKEN` and the GitHub Actions app. Only this job gets `statuses: write`.
The status is success only when the aggregate job succeeded, otherwise failure,
and links to the workflow run. The command refuses other events, execution
outside GitHub Actions, and the `dev`/`main` refs.

A head that never moves, a carry error or a dispatch error
adds a failure comment and fails the updater run. The event-driven carry job
still serves pushes made by `make ship`.

Before every push, including without SHIP_FORCE and with an unchanged dev, ship fetches the branch again. If its only additions since
the ship's starting head are two-parent merges committed by `GitHub
<noreply@github.com>`, each with a dev second parent, it prints one line and
leases on the fetched head. Other remote changes refuse the push; the lease
still rejects a push that races this final fetch. These validation fetches
disable remote-tracking updates, so a refused attempt cannot authorize the
same overwrite on an unchanged retry. Without force, ordinary fast-forwards
remain allowed; other divergence still refuses. Updater output says
`updated (dev merged in)`.

## What only GitHub can show

The commands are tested against stubbed `gh` and real Git (`cargo test`, the
`ship::postpush` tests). After merge, on a real pull request: that
`pull_request_target` runs from `dev`, the workflow token and permissions suffice,
that `updatePullRequestBranch` merges `dev` in and the explicit dispatch commit status
is accepted as `required` from the Actions app, that the carried
status is accepted for `harness/review`, and what `behind_by` reports.

The dispatched status satisfying branch protection is verified only after merge
on a real pull request; local stub tests do not verify that GitHub behavior.
