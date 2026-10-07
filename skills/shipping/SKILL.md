# Shipping a change

Read [shipping](../../docs/shipping.md) and [review](../../docs/review.md).

1. Commit on a feature branch; run `make review-floor` and pick the tier (never
   below it; higher for risky changes). Write the description file.
2. `make ship SHIP_TITLE=… SHIP_BODY=<file> SHIP_TIER=<tier> SHIP_RUNTIME=<your runtime>`.
3. Review below 8/10: fix and commit every confirmed finding (never weaken a
   test or gate), file out-of-scope ones as issues, ship again.
4. After three failed full reviews `make ship` decides, never a person: it
   runs up to two **closing reviews** of your fixes (commit them first), then
   accepts a **final fix** (≤ 150 changed lines, each fix with a test that
   fails without it; open an issue for a post-merge review). If the findings are not converging it tells
   you to **split** the change into smaller pull requests: do it, close the
   original, file leftovers as issues. Never rerun reviews to fish for a grade,
   lower the tier, or edit reviewer prompts or `gates/review.json` in the same
   branch.
5. Report the pull request URL, the grade and what is left in issues.
