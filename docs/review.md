# Adversarial review

No person reviews AoeWorld pull requests. Instead, every change gets a tiered
adversarial review of its exact commit, graded /10:

```sh
make ship SHIP_TITLE='…' SHIP_BODY=<description file> SHIP_TIER=medium SHIP_RUNTIME=claude
make review REVIEW_TIER=medium REVIEW_RUNTIME=claude REVIEW_TASK=<description file>   # review only
target/debug/aoe-harness review --explain   # stored-review decision only, no review or approval write
```

`make ship` runs the review of the exact commit after the gates. Into `dev`,
it first pushes a draft PR so CI runs alongside review; approval marks it ready
and arms auto-merge ([shipping](shipping.md)).

`make review-floor` prints the lowest tier the change allows. The agent picks the
tier (never below the floor) and says why in the description. A full review passes at
7/10 or more with no blocking finding: no confirmed major or critical, and no disputed
critical (a disputed finding counts one severity lower). The bar is 7 since 2026-10-19.

## Tiers

`gates/review.json` holds the tiers, the model family of each runtime and the
floors. Reviewers use the model family of the runtime you are working in
(`REVIEW_RUNTIME`; set it, the default is `claude`): Claude reviews in Claude Code, GPT in Codex, GLM in DeepSeek
Harness and pi.

Low uses one quick reviewer; medium uses correctness and spec; high, xhigh and max add hostile-input. Low through high use the average models (low may use light, high raises effort); xhigh and max use strong. Low has one round, medium through xhigh up to three, and max up to five. The correctness persona also checks test integrity: weakened or deleted assertions, tests changed to match buggy output, added skips or ignores, `allow` added to silence a lint, and thresholds or baselines loosened without a stated reason. Codex uses GPT-6.1 Terra for average and strong. After capacity or rate limits persist through the 60- and 180-second retries, Codex and Claude sessions try the other runtime once at the same strength; reports record `runtime_fallback`.

The **light** model is the one exception to the average and strong levels. It
is per runtime family and used only by tier low's reviewer and grader sessions
(`light` in `gates/review.json`; a family without it keeps its average model).
Closing reviews, the arbiter and every other tier never use it. Claude Haiku
5.5 (medium) is light for Claude, on `make review-calibrate` over the 30 seeds at
tier low, medium effort:

| Model | Planted defects blocked | Clean cases wrongly blocked | Mean grade, clean | Mean grade, planted |
|---|---|---|---|---|
| Sonnet 5.5 (average) | 20/20 | 0/10 | 9.2 | 3.5 |
| Haiku 5.5 (light) | 20/20 | 0/10 | 8.6 | 3.8 |

The seed set is too easy to separate the two above that tier, so Haiku is used
for low only; medium, high and max keep their models. This is a limit: the set
shows no recall gap, so it cannot tell whether Haiku misses subtler defects
than Sonnet does; widening the seeds is tracked in #377.

Floors: docs-only and static changes allow `low`; native, gameplay, browser,
asset, performance and map/geodata changes need `medium` (map and geodata still
select every product suite in CI); the harness, gates, instructions and
`.github/` (`everything`) need `high`.

## Threat model

Reviewers judge against [CLAUDE.md's threat model](../CLAUDE.md#threat-model),
which the shared preamble states: realistic agent misbehaviour through ordinary
commands or edits (skipped steps, shortcuts, weakened tests, edits outside a
role, mistakes) is in scope at full severity; a deliberate, contrived attack on
a guard, by any agent, is a documented limit, reported at most as `minor`.
Without it, reviewers ask for ever more mechanism against a determined attacker
that no local check can stop, and reviews stop converging.

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
5. **Arbiter** (tier high and above; full reviews only). After the rounds, a
   fresh single session on the runtime's strong model (`AOE_AGENT_ROLE=reviewer`,
   read-only, prompt `gates/review/arbiter.md`) sees every finding of severity
   `major` or `critical` that is still `disputed` with the votes and their
   evidence, and rules each `upheld` or `refuted`, citing code. Its ruling sets
   the status (`confirmed` at the reported severity, no `partial` downgrade, or
   `refuted`) and is stored with its reason as a vote of the reserved reviewer
   `ARBITER`; the report shows it as `arbiter: …` under the finding. If the
   arbiter gives no usable answer (after the one retry every session gets), or
   skips a finding, that finding stays `disputed`, as at the lower tiers; the
   review is still complete. Closing reviews have no arbiter: their votes ask
   whether a fix landed, not whether a problem is real.
6. **Grade.** A fresh grader session writes a grade /10 and at most two plain
   lines. Rust caps it: a confirmed critical finding, or a major or critical
   test-integrity one (weakened or gamed tests), caps it at 4; a confirmed
   major one at 7. A minor test-integrity finding (a small coverage gap) counts
   by its severity, like any other. A **disputed** finding (votes split) counts
   one severity lower, so a single dissenting reviewer cannot erase a critical
   one.

**One finding per defect.** After the blind round and after every later round,
findings that describe the same defect are folded into the earliest of them
(`review/cluster`): same file, lines within 3 of each other, and claims sharing
at least half the shorter claim's words (or 30% plus the same identifier, such
as an environment variable). It is a plain text comparison; no model is asked,
and findings on different files or far-apart lines never merge. The cluster
counts at the highest severity of its members and keeps the test-integrity
category of any member, so the grade caps mean what they did. Its reporters
never vote on it, later votes apply to the cluster, and the report, the
caps and the follow-up issues count clusters. Nothing is dropped: the report
lists each merged report under its cluster as "also reported as".

A full review **passes** when every session answered and the capped grade is at
least `merge_grade` (7) and no blocking finding stands (a confirmed major or critical, or a
disputed critical). Every report is stored, keyed by commit, tier and
attempt, in `<git common dir>/aoe-ship/reviews/` (agents cannot write there);
none replaces another.

## Calibration

`make review-calibrate [CALIBRATE_TIER=low] [CALIBRATE_RUNTIME=claude]
[CALIBRATE_CASES=n]` (main session only) measures the reviewers instead of a
change. `gates/review/seeds/<area>/<case>.json` holds about 45 small cases, a
third clean refactors and features and the rest with planted defects:
`correctness` (off-by-one, overflow, wrong boundary), `test-integrity`
(loosened assertion, `#[ignore]`, special-cased test), `spec` (requirement
unmet, scope creep), `hostile-input` (unbounded read, panic on malformed data)
and `performance` (quadratic loop on a hot path). A case is a base file set, a
unified diff, the task sentence and the expectation: clean, or one or more
planted defects, each naming its file, its line range in the file as the diff
leaves it, and the severity a correct review assigns. Each case becomes a
temporary git repository (policy files on its `origin/dev`, the diff as one
commit) and gets the normal review at the chosen tier and runtime, outside
the branch review budget. `CALIBRATE_CASES` takes a sample round-robin across
the areas.

The harder cases of #377 live one level deeper, under
`gates/review/seeds/hard/<area>/`: an off-by-one at a chunk boundary whose
wrong number is produced in one file and consumed in another; an ordering bug
(events applied before the state they depend on); a defect that appears only
when two changes in one diff interact; a test that passes for the wrong
reason; a silent behaviour change hidden in a refactor; a determinism break
(`HashMap` iteration feeding a simulation result); three cases with two
defects of different severity (partial recall); and five clean changes that
look suspicious (a deliberate wrapping add, a removed check with the reason
in the task, a mechanical rename, a performance rewrite with an equivalence
test, a deliberately ignored error with a justification). `make test-harness`
validates every seed: the base compiles with its tests, the diff applies, and
the tests stay green through it (a defect the seed's own tests catch would
not be hard to review). Separate input probes exercise each Rust base and
post-diff crate, checking ordinary inputs and pinning known planted failures;
these small probes do not establish exhaustive correctness or performance.

It writes `reports/review/calibration.json` and `.md`: per area the planted
defects found (recall: a standing finding of at least the planted severity,
on the defect's file within its lines, so a case with two planted defects can
be half found; each finding matches at most one defect), per persona the
number of distinct planted defects it reported (several may come from one
case) and what it raised on clean cases, and the clean cases wrongly blocked
(a complete review that does not pass) with their grades. Cases whose review did not complete count as "no
result", not as misses. It is a tool for tuning prompts, personas and models,
not a gate, and a case set this small shows large differences only.

## What happens next

- **Pass:** `make ship` publishes once the parent is ready: it pushes, appends
  the review to the pull request and posts `harness/review` = success. Auto-merge
  is armed only into `dev`, whose protection requires both `required` (CI)
  and `harness/review`. A local stacked parent needs no pull request for
  review: `make ship SHIP_BASE=<parent> SHIP_BODY=<description file>` runs the
  gates and review against its local tip, records that parent commit, and
  stops before publishing with `reviewed ahead: <grade>; push when the parent
  <branch> has its pull request`. `make review SHIP_BASE=<parent>` also runs the
  gates before reviewing or loading a stored approval; it never publishes.
  Once the parent has an open harness pull request, ship against its fetched
  origin tip. After it merges, rebase onto `origin/dev` and ship with
  `SHIP_BASE=dev` ([shipping](shipping.md#stacked-pull-requests)).
- **Approval follows the patch.** A stored passing review can answer for the
  same commit or a rebased commit on the same branch, even after parent edits
  or a change of base. The harness recomputes the branch's own diff from its
  merge base with the **recorded base commit**, and compares it with the diff
  against the selected base now. Per file, identity retains the byte-exact
  path, old/new modes, ordered removed and added line bytes, and final-newline
  markers. Context, line offsets and hunk grouping are ignored, so unrelated
  edits to a shared file do not discard approval. Binary changes and gitlinks
  require identical old/new object IDs. Unlike `git patch-id --stable`, this
  keeps literal whitespace changes distinct. Conflict resolution that changes the patch
  requires another review. The stored SHA-256 fingerprint is informational;
  equality is checked from Git, never trusted from the report.
  Only the same branch name qualifies, at the required tier or higher.
  **An unchanged approved patch stays approved** whatever the description:
  the newest passing, complete, non-reused review of the branch whose own
  patch is identical to the current one (each against its own base), whose
  policy fingerprint is unchanged and whose tier is not below the floor or
  the requested tier, is reused when only the `SHIP_BODY`/`REVIEW_TASK` text
  (or the commit log standing in for it) changed. The reuse report records
  the new task fingerprint and the note `description text changed after
  approval; code unchanged`, and no reviewer runs. This is the same code
  path as every other reuse, not a second copy: the task text is compared
  only after the patch is known to be identical, so where the patch differs
  the description never decides anything (the increment below applies). The
  same commit shipped again under another description (also a second edit,
  after an earlier reuse) keeps its approval the same way: its passing,
  complete report is stored again under the new task fingerprint with the
  same note, and no reviewer runs. A failed report of that commit, or a
  changed policy or tier, still means a review, and the `review:` line names
  that first failing condition.
  Another branch's identical patch never answers this branch's findings.
  A complete report for the target head against the current base blocks
  copying another review over it, including a failing verdict.
  A review pins one policy commit (the judge's `AOE_JUDGE_REV`, else
  `origin/dev`) before reading config, registry, models and prompts. Reuse
  requires that pinned commit to be current `origin/dev` and the source's
  policy fingerprint to match. Floor, eligible tiers and merge grade come
  from that commit. Reuse writes a report for the current head and base,
  pointing to the original passing full or closing review; a reuse report
  never becomes a source. The status and PR description say
  `review of <sha> reused (same patch)` (and the note, when the description
  changed).
- **Every ship says why.** `make ship` and `make review` print one line
  before any reviewer starts, record it in the report and put it in the PR
  description: `review: reusing 9/10 of 76994762b3c1 (patch and policy
  unchanged; description text changed)`, `review: closing review of 12 lines
  since 8107b508a0d2, approved 9/10` (an increment), or `review: full review
  — <reason>`. When a passing review exists but cannot be used, the reason is
  the first failing condition: `the patch differs from the 9/10 of <sha>`,
  `the <g>/10 of <sha> is a <tier> review and this ship needs at least
  <tier>`, `the review policy on dev changed since the 9/10 of <sha>`,
  `the base commit of the 9/10 of <sha> is missing`, `N stored review
  report(s) are unreadable and no readable passing review of this branch
  exists`; otherwise `no passing review of this branch`. Unreadable reports
  count only when their header identifies this branch or their filename identifies
  one of its commits (including pre-rebase reflog tips), not shared base ancestors.
  Gates **always run before reuse** on the current tree, and CI runs them
  again. The same patch can behave differently after a parent deletes code it
  relies on; patch identity does not establish semantic equivalence.
- Reviewer processes that fail with a rate-limit diagnostic get at most three
  attempts, waiting 60s then 180s. Other process failures are not retried.
  On Codex or Claude, exhaustion tries the other runtime once; failure there
  is incomplete. The retry for an unreadable answer is separate.
- **Fail:** into `dev`, the PR stays a draft with its CI state printed; other
  paths publish nothing. Fix every confirmed finding without weakening a
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
Repeated full reviews found different edge cases instead of checking the fixes,
costing one to two hours per round; one repeatedly rejected change then passed
its first closing review. Rerunning until a grade crosses 8 would
measure luck, not quality. So a branch has one full blind review, then a bounded
budget for checking fixes. `make ship` decides from the stored reports
(`crates/harness/src/review/closing.rs`):

1. **One full blind review.** If it passes, the change can ship. If it fails,
   fix and commit every confirmed finding. Every review is stored (none
   replaces another) and a commit that failed a review is never reviewed
   again; only new commits are. An
   **incomplete** review (a session never answered) checked nothing: it does
   not count and the same commit may be reviewed again. Three incomplete
   reviews within an hour mean the reviewers are unavailable: `make ship`
   refuses for the rest of that hour, then reviews again.
2. **Every later review of the same code is a closing review.** There is no
   second full blind review of it (new commits after a pass are another matter:
   [below](#commits-after-an-approval)). A severe finding of the full review (critical, or a major test
   weakening: the harness's `severe` predicate) is carried like any other
   blocking finding: such a finding is usually a small fix, and the closing
   review must show it fixed.
3. **Up to three closing reviews.** Each runs on the **strong**
   models with at least two reviewers (a one-persona tier borrows the medium
   tier's) and skips the blind round. Every reviewer votes on each blocking
   finding not yet shown fixed (`upheld` = still there, `refuted` = fixed,
   citing the fix) and audits only the branch's own changes since the last
   reviewed commit; new findings must be in that diff and get a second round.
   A closing review **passes when no blocking finding is left, no carried
   finding is left undecided (disputed)** and every session answered; its grade
   is reported, not gated. A carried finding confirmed only as minor (`partial`
   votes count one severity lower) no longer blocks, as in a full review. A
   carried finding still blocking or undecided, or a severe finding in the
   closing review, means split at once. If only new non-severe blocking
   findings in the fixes remain, fix and commit them for another closing
   review within the budget.
4. **Budget spent: split.** If the third closing review fails, split.
5. **A passing review is final for the code it passed.** It settles the
   earlier history: the findings and the budget of everything before it are
   closed, and the code it approved is never reviewed again, by a full review
   or otherwise. A commit added after it starts a new increment with a fresh
   budget (one first review, then up to three closing reviews of its fixes),
   and the first review judges **only the diff from the approved state to
   HEAD** ([below](#commits-after-an-approval)).

### Commits after an approval

The *approved state* is the newest passing full or closing review of the
branch (a reuse report never counts) that `make ship` can place in the branch
now, in this order:

1. its commit is in HEAD's history: the increment is the diff from that commit
   to HEAD (only the branch's own files, so a rebase or a
   merge of `dev` does not bring dev's changes in);
2. after a rebase, a commit of the branch (up to 100) whose own patch is
   byte-identical to the approved one by the
   [patch identity](#what-happens-next): the increment is the diff from that
   commit to HEAD;
3. after a rebase that changed the patch by conflict resolution, the files
   whose changed lines differ from the approved patch (or that it did not
   have): their whole diff against the merge base is the increment, so the
   changed hunks are always reviewed (coarser than a hunk, never looser);
4. otherwise, or when a stored report cannot be read or a file name cannot be
   named exactly, nothing is approved and the whole diff gets a full review.

A rebase that leaves the approved patch unchanged keeps
[reuse](#what-happens-next) and has no increment to review.

The increment gets a **closing-style** review (strong models, at least two
reviewers, no blind round, a pass on "no blocking finding left") when it is at
most 200 changed lines **and** the floor of the files it changes is not above
the tier of the passed review. Otherwise it gets a **full** review of the
increment at the tier you chose (never below the floor of the whole change).
Both judge the increment's diff, and the approved diff is only read-only
context. **A finding must cite a line inside the increment's hunks**, or name
a file the increment changes without a hunk to cite (deleted, renamed, mode
changed or binary; for a rename, the old path too), where any line or none is
inside; anything else is recorded as a note (`Notes outside the increment`, in the report and
the pull request) and can never block, whatever its severity: the code is
approved. Every line of every new commit is still reviewed; the reviewers
cannot skip one. The report and the pull request's review line say which ran:
`closing review of 12 lines since <sha>, approved 9/10`.
Small follow-ups add up: the 200 lines count everything approved by closing-style
increment reviews since the last passing full review, however it came to be
approved, so a large change cannot be added in small slices without a full
review of the slice that crosses the limit. Walking the history forward, a
passing full review resets the total; a passing closing-style increment review
adds its lines; a failed one is pending (the latest wins) until a passing
closing review of its fixes adds those lines, and a failed full review of an
increment, once its fixes pass, resets the total (it had a blind review). A
reused copy of a report never counts again.

Budget: the pass closed the earlier budget, so the first review of an increment
takes the full review's slot, whatever its style, and its failures are
followed by up to three closing reviews of the fixes, then a split, exactly as
for a new branch. A branch ends in a pass or a split: every increment either
passes, or spends at most four reviews. Nothing asks a person.

To **split**: break the change into smaller pull requests on new branches
(each with its own budget, smaller diffs review better), close the original,
and file what is left as issues. Not allowed: another full review to fish for
a grade, a lower tier or effort, or changing reviewer prompts or
`gates/review.json` in the same branch (they are read from `origin/dev`
anyway).

Why this policy, and not the alternatives:

| Option | Verdict |
|---|---|
| Merge when the budget is spent anyway | No: the last review's blocking findings would ship unfixed. |
| Full reviews until a grade ≥ 8 | No: grades are noisy and each fix adds new surface, so it rewards luck and never has to end. |
| Lower effort until convergence | No: a weaker review converges by seeing less. |
| A higher tier, full scope | Costly, and it repeats the blind round over code already reviewed once. |
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
- Reviewers can be slow: a high-tier review can require multiple sessions.
- Prompts are passed as one command-line argument, so each is kept under
  120 KB: the diff is cut first (reviewers read the rest with `git diff`).
