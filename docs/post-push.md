# After the push

`make ship` pushes a reviewed commit and arms auto-merge. While the pull request
waits for CI, `dev` may move, and GitHub would merge the pull request untested
against the new `dev` (four incidents in two days, #388). Three pieces close
that window; the workflow `.github/workflows/post-push.yml` only decides when
they run, the logic is in `crates/harness/src/ship/postpush/`.

**The setting is off.** "Require branches to be up to date before merging" for
`dev` (`strict: true`) is switched on by the owner after this has been seen
working on one real pull request. Until then the updater and the carry run but
nothing forces GitHub to wait for them; no repository setting, branch
protection or ruleset is changed by the code.

## 1. The marker

When `make ship` (or `make review-pr`) posts `harness/review` into `dev`, it
also writes a pull request comment that starts with the hidden line
`<!-- aoe-harness:review-marker -->`, then one JSON comment line: the approved
`head`, its own-patch `patch` identity (SHA-256 of the exact changed bytes
against the base, as `review/reuse.rs` computes it, recomputed from Git), the
review `policy` fingerprint (`Report::policy_fingerprint_at`), `tier` and
`grade`. The next ship edits that comment in place; if it cannot, it adds one.
A comment, not the status description: a description holds 140 characters and
the two fingerprints are 128. A marker that cannot be written is a warning;
the ship still succeeds, only a later rebase will not carry the review.

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
or `no review marker`. It never posts success for a head whose own patch
differs. This is the rule of the local re-check carry (#367) and of the
unchanged-patch reuse (#386), run where the rebase happens. Token: `statuses:
write` and `pull-requests: write`.

## 3. Updating waiting pull requests (`make update-prs`)

On every push to `dev`: each open pull request into `dev` with auto-merge armed
and behind `dev` (GitHub's compare: `behind_by > 0`) is rebased with the GraphQL
`updatePullRequestBranch`, method REBASE (linear history requires it), pinned
to the head the run saw. At most `UPDATE_MAX_PRS` (5) pull requests per run,
oldest first; one run at a time (concurrency group `post-push-update`, never
cancelled halfway). A rebase that GitHub refuses with conflicting files leaves
the pull request untouched and adds one comment naming the files (once per
head); a refusal with no conflict is an error of the run. Forks are skipped.

The rebase must be pushed with a personal token: a push made with the workflow's
own `GITHUB_TOKEN` starts no CI run, and the rebased pull request would wait
for a `required` check forever. The workflow reads it from the repository
secret `PR_UPDATE_TOKEN` (the owner creates it). Without it the job prints a
notice and does nothing, so the updater is inert until then.

## What only GitHub can show

The commands are tested against stubbed `gh` and real Git (`cargo test`, the
`ship::postpush` tests). After merge, on a real pull request: that
`pull_request_target` runs from `dev`, the secret and the permissions suffice,
that `updatePullRequestBranch` rebases and triggers `required`, that the carried
status is accepted for `harness/review`, and what `behind_by` reports.
