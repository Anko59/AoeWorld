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
  passing review is reused by a later `make ship` of the same commit. Branch
  protection on `dev` requires `required` (CI) today; adding `harness/review` to
  the required checks is a pending repository setting, to be made once every
  agent session ships through `make ship`.
- **Fail:** nothing is pushed. Fix every confirmed finding without weakening a
  test or gate, commit, and ship again. File what you leave out of scope as an
  issue. The branch's review budget is below.

## When reviews do not converge

Agents run this project without a person: no rule here ends in "ask a
person", and every branch reaches either a merge or a split on its own.

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
   is too big to fix by iteration. Split it into smaller pull requests on new
   branches (each with its own budget), close this one, and file what is left
   as issues.
3. **Converging: up to two closing reviews.** Each runs at the same tier on
   the **strong** models and skips the blind round. Every reviewer votes on
   each blocking finding found so far (`upheld` = still there, `refuted` =
   fixed, citing the fix) and audits only the diff since the last reviewed
   commit; new findings must be in that diff. It always has at least two
   rounds, so what it finds is cross-examined. A closing review **passes when
   no blocking finding is left, every carried finding was shown fixed
   (refuted)** and every session answered; its grade is reported, not gated. If it finds an earlier finding still open (a fix that
   did not fix) or a critical one, split as in 2.
4. **Final fix.** When both closing reviews failed only on new findings in
   the fixes, fix those, each with a test that fails without the fix, and ship:
   that commit merges on tests and gates (`Final fix` in the pull request), with
   no further model review. `make ship` refuses a final fix of more than 150
   changed lines since the last closing review (a binary file counts as over),
   or one without a test file.
   Open an issue asking for a post-merge review of it.

The budget bounds the cost at five reviews plus one small unreviewed fix, and
each step looks at less code than the one before. Not allowed: another full
review to fish for a grade, a lower tier or effort, or changing reviewer
prompts or `gates/review.json` in the same branch (they are read from
`origin/dev` anyway).

Why this policy, and not the alternatives:

| Option | Verdict |
|---|---|
| Merge after the third review anyway | No: the last review's blocking findings would ship unfixed. |
| Full reviews until a grade ≥ 8 | No: grades are noisy and each fix adds new surface, so it rewards luck and never has to end. |
| Lower effort until convergence | No: a weaker review converges by seeing less. |
| A higher tier, full scope | Costly, and it repeats the blind round over code already reviewed three times. |
| **Strong models on the fixes plus the known findings** | Yes: bounded cost, the strongest check where the risk is (the fixes), and a criterion that does not depend on grade noise. |
| **Final fix on tests and gates** | Yes, last: by then the findings are confined to small fix diffs, and a test per fix proves it; a post-merge review issue covers the rest. |
| **Split a diverging change** | Yes: smaller diffs review better, and the split pieces start fresh. |
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
