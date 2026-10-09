# Adversarial review

No person reviews AoeWorld pull requests. Instead, every change gets a tiered
adversarial review of its exact commit, graded /10:

```sh
make ship SHIP_TITLE='…' SHIP_BODY=<description file> SHIP_TIER=medium SHIP_RUNTIME=claude
make review REVIEW_TIER=medium REVIEW_RUNTIME=claude REVIEW_TASK=<description file>   # review only
```

`make ship` runs the review of the exact commit after the gates and before
anything is pushed.

`make review-floor` prints the lowest tier the change allows. The agent picks the
tier (never below the floor) and says why in the description. A review passes at
8/10 or more.

## Tiers

`gates/review.json` holds the tiers, the model family of each runtime and the
floors. Reviewers use the model family of the runtime you are working in
(`REVIEW_RUNTIME`; set it, the default is `claude`): Claude reviews in Claude Code, GPT in Codex, GLM in DeepSeek
Harness and pi.

| Tier | Reviewers (personas) | Rounds | Models |
|---|---|---|---|
| low | 1 (quick: correctness + test integrity) | 1 | average (Sonnet 5.5 medium, GPT‑6 Luna xhigh, GLM 5.3 Flash high) |
| medium | 3 (correctness, spec, test integrity) | ≤ 3 until convergence | average |
| high | 5 (adds hostile input, performance) | ≤ 3 | average, reasoning one step higher |
| xhigh | 3 | ≤ 3 | strong (Opus 5.5, GPT‑6.1 Sol, GLM 5.3; medium) |
| max | 5 | ≤ 5 | strong |

Floors: docs-only and static changes allow `low`; native, gameplay, browser,
asset and performance changes need `medium`; the harness, gates, instructions,
`.github/` and the map/geodata crates (`everything`) need `high`.

## Threat model

Reviewers judge against [CLAUDE.md's threat model](../CLAUDE.md#threat-model),
which the shared preamble states: an escape by a subagent is in scope at full
severity; a main session deliberately defeating a guard is a documented limit,
reported at most as `minor`. Without it, reviewers ask for ever more mechanism
against a local forger that no local check can stop, and reviews stop
converging.

## Protocol

The personas, preamble and grader prompts live in `gates/review/`. They, the
tiers, floors and merge grade (`gates/review.json`) and the registry that sets
the floor are read from `origin/dev`, never from the branch under review: a
change to them takes effect once merged. Every
reviewer is a fresh headless session of the runtime, launched as
`AOE_AGENT_ROLE=reviewer` (the hooks keep it read-only), with the tier's model
and effort (`crates/harness/src/review/`).

1. **Round 1, blind.** Each reviewer sees the task (the PR description), the
   frozen diff against the merge base and deterministic facts (test/code line
   split; added `#[ignore]`, `unsafe`, `.unwrap(`, `cfg(test)`; changed CI,
   gate or baseline files). It never sees the author's reasoning. Every finding
   must be falsifiable: file, line, trigger, expected versus actual, evidence.
   An empty list is a good answer.
2. **Rounds 2+, cross-examination.** Each reviewer gets the others' findings,
   anonymized and shuffled, and votes on each: `upheld`, `partial`, `refuted`
   (citing code) or `unverifiable`, on every finding shown: a skipped vote or a
   malformed answer makes the review incomplete. New findings are accepted only
   if major or critical, and not in the last round (they could never be
   cross-examined). This is per-finding verdicts, not open debate.
3. **Status, computed in Rust.** Each other reviewer's latest vote counts
   once (`partial` upholds, `unverifiable` abstains): more upheld than refuted is
   *confirmed*, more refuted is *refuted*, otherwise *disputed*. A single
   reviewer's findings stand as confirmed. A finding that full `upheld` votes
   alone would not confirm counts one severity lower (`partial` means "real,
   but a different severity or scope").
4. **Stop** when a round changes no status and adds no finding, when it adds
   no finding and leaves none disputed (asking again would repeat the same
   question), or at the tier's round cap.
5. **Grade.** A fresh grader session writes a grade /10 and at most two plain
   lines. Rust caps it: a confirmed critical finding, or a major or critical
   test-integrity one (weakened or gamed tests), caps it at 4; a confirmed
   major one at 7. A minor test-integrity finding (a small coverage gap) counts
   by its severity, like any other. A **disputed** finding (votes split) counts
   one severity lower, so a single dissenting reviewer cannot erase a critical
   one.

The review **passes** when every session answered and the capped grade is at
least `merge_grade` (8). Every report is stored, keyed by commit, tier and
attempt, in `<git common dir>/aoe-ship/reviews/` (agents cannot write there);
none replaces another.

## What happens next

- **Pass:** `make ship` pushes, appends the review to the pull request, posts
  the commit status `harness/review` = success and arms GitHub auto-merge. A
  passing review is reused by a later `make ship` of the same commit. After a
  clean rebase, it may also be reused when Git's raw
  `diff --raw --no-abbrev -z --no-renames <merge-base>..<commit>` output is
  byte-identical for both commits. This means the same paths go from identical
  old blobs and modes to identical new blobs and modes; the report stores a
  SHA-256 fingerprint for information, while reuse recomputes both sides from
  Git. Any complete report for the target commit at the required tier or higher
  blocks reuse, whether it passed or failed. Only a review recorded for the
  same branch name is a source, under the reviewer policy that `origin/dev`
  holds today: before loading its model, a review pins one policy commit
  (the judge's own, `AOE_JUDGE_REV` from `.agents/hooks/harness.sh`, else
  `origin/dev` once), reads config, model and prompts from it and stores its
  fingerprint. Reuse writes a new harness report
  for the rebased commit that points to the original full
  or closing review, and the status and PR description identify the reuse. A
  reuse report is never itself a source for another reuse. Failing reviews and
  reviews below the required tier are not reused. This is a local review
  equivalence check: a patch can behave differently on a new base, so CI
  re-runs every gate on the new commit. Branch
  protection on `dev` requires both `required` (CI) and `harness/review`, so
  nothing merges without a passing review (Dependabot PRs need a review path:
  #161).
- **Fail:** nothing is pushed. Fix every confirmed finding without weakening a
  test or gate, commit, and ship again. File what you leave out of scope as an
  issue. The branch's review budget is below.

## Pull requests the harness did not open

`make review-pr PR=<n>` (main session only) reviews a **Dependabot** pull
request based on `dev`, which needs the required `harness/review` status. It
reviews that exact head commit in a temporary detached worktree, with the
normal per-branch review budget and `REVIEW_RUNTIME` family; dependency
major-version changes need at least `medium`. Because the reviewers run in the
PR's own tree, only a change that touches nothing but dependency manifests,
lockfiles, `docker/*.Dockerfile` and `.github/workflows/` is reviewed: it
cannot alter agent, hook or harness configuration. Other authors, forks and
other bases are refused.

A passing review is published only if the PR still has the reviewed head and
base: the report as a PR comment, `harness/review` = success, and squash
auto-merge pinned to that commit (`--match-head-commit`). A failed or
incomplete review publishes nothing. Reuse after a rebase for `make review-pr`
is planned in issue #146. `REVIEW_PR` reaches the harness through Make's
environment, never shell text.

## When reviews do not converge

Agents run this project without a person: no rule here ends in "ask a
person". Every branch ends in a **reviewed merge or a split**, never in code
that no review passed.

Each full review reads the whole diff blind, so every fix adds new surface to
find fault with, and grades vary by several points between identical runs.
Rerunning until a grade crosses 8 would measure luck, not quality. So a branch
has a budget, and `make ship` decides from the stored reports
(`crates/harness/src/review/closing.rs`):

1. **Up to three full reviews.** After each, fix and commit every confirmed
   finding. Every review is stored (none replaces another) and a commit that
   failed a review is never reviewed again; only new commits are. An
   **incomplete** review (a session never answered) checked nothing: it does
   not count and the same commit may be reviewed again. Three incomplete
   reviews within an hour mean the reviewers are unavailable: `make ship`
   refuses for the rest of that hour, then reviews again.
2. **Not converging: split.** If the last full review confirmed a finding
   that caps at 4 (critical, or a major test weakening), or more blocking
   findings (those capping below 8) than the full review before it, the change
   is too big to fix by iteration.
3. **Converging: up to two closing reviews.** Each runs on the **strong**
   models with at least two reviewers (a one-persona tier borrows the medium
   tier's) and skips the blind round. Every reviewer votes on each blocking
   finding not yet shown fixed (`upheld` = still there, `refuted` = fixed,
   citing the fix) and audits only the branch's own changes since the last
   reviewed commit; new findings must be in that diff and get a second round.
   A closing review **passes when no blocking finding is left, no carried
   finding is left undecided (disputed)** and every session answered; its grade
   is reported, not gated. A carried finding confirmed only as minor (`partial`
   votes count one severity lower) no longer blocks, as in a full review. A
   carried finding still blocking or undecided, or a critical one, means split.
4. **Budget spent: split.** If both closing reviews failed, split.

To **split**: break the change into smaller pull requests on new branches
(each with its own budget, smaller diffs review better), close the original,
and file what is left as issues. Not allowed: another full review to fish for
a grade, a lower tier or effort, or changing reviewer prompts or
`gates/review.json` in the same branch (they are read from `origin/dev`
anyway).

Why this policy, and not the alternatives:

| Option | Verdict |
|---|---|
| Merge after the third review anyway | No: the last review's blocking findings would ship unfixed. |
| Full reviews until a grade ≥ 8 | No: grades are noisy and each fix adds new surface, so it rewards luck and never has to end. |
| Lower effort until convergence | No: a weaker review converges by seeing less. |
| A higher tier, full scope | Costly, and it repeats the blind round over code already reviewed three times. |
| A final fix merged on tests alone | No: it is unreviewed code, and every bound on it (size, tests, files) proved gameable in review. |
| **Strong models on the fixes plus the open findings** | Yes: bounded cost, the strongest check where the risk is (the fixes), and a criterion that does not depend on grade noise. |
| **Split what still does not pass** | Yes: smaller diffs review better, and the pieces start fresh. |
| Ask a person | No: agents work autonomously; the policy must end on its own. |

## Limits

- Grades are model judgements. The protocol makes them harder to game (blind
  first round, code-cited refutations, caps from confirmed and disputed
  findings), but a
  shared blind spot of one model family can survive; mix families when usage
  allows.
- The `harness/review` status is posted with the person's GitHub token by the
  harness. A same-user process could forge it; CI re-runs every gate.
- Reviewers can be slow: a high-tier review is up to 16 sessions.
- Prompts are passed as one command-line argument, so each is kept under
  120 KB: the diff is cut first (reviewers read the rest with `git diff`).
