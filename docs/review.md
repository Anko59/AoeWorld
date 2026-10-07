# Adversarial review

No person reviews AoeWorld pull requests. Instead, every change gets a tiered
adversarial review of its exact commit, graded /10:

```sh
make review REVIEW_TIER=medium REVIEW_RUNTIME=claude REVIEW_TASK=<description file>
```

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
4. **Stop** when a round changes no status and adds no finding, or at the tier's
   round cap.
5. **Grade.** A fresh grader session writes a grade /10 and at most two plain
   lines. Rust caps it: a confirmed critical finding, or a major or critical
   test-integrity one (weakened or gamed tests), caps it at 4; a confirmed
   major one at 7. A minor test-integrity finding (a small coverage gap) counts
   by its severity, like any other.

The review **passes** when every session answered and the capped grade is at
least `merge_grade` (8). Every report is stored, keyed by commit, tier and
attempt, in `<git common dir>/aoe-ship/reviews/` (agents cannot write there);
none replaces another.

## Limits

- Grades are model judgements. The protocol makes them harder to game (blind
  first round, code-cited refutations, caps from confirmed findings), but a
  shared blind spot of one model family can survive; mix families when usage
  allows.
- Reviewers can be slow: a high-tier review is up to 16 sessions.
- Prompts are passed as one command-line argument, so each is kept under
  120 KB: the diff is cut first (reviewers read the rest with `git diff`).
