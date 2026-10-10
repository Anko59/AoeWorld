# Shipping a change

Read [shipping](../../docs/shipping.md) and [review](../../docs/review.md).

1. Commit on a feature branch; run `make review-floor` and pick the tier (never
   below it; higher for risky changes). Write the description file.
2. `make ship SHIP_TITLE=… SHIP_BODY=<file> SHIP_TIER=<tier> SHIP_RUNTIME=<your runtime>`.
   A change that depends on an open harness pull request ships stacked on it
   with `SHIP_BASE=<its branch>` (no auto-merge yet); after that parent merges,
   rebase onto `origin/dev` and ship again with `SHIP_BASE=dev`
   ([stacked pull requests](../../docs/shipping.md#stacked-pull-requests)).
3. Review below 8/10: fix and commit every confirmed finding (never weaken a
   test or gate), file out-of-scope ones as issues, ship again.
4. After three failed full reviews `make ship` decides, never a person: it
   runs up to two **closing reviews** of your fixes (commit them first). If the
   findings are not converging, or the closing reviews do not pass, it tells
   you to **split** the change into smaller pull requests: do it, close the
   original, file leftovers as issues. Never rerun reviews to fish for a
   grade, lower the tier, or edit reviewer prompts or `gates/review.json` in
   the same branch.
5. Report the pull request URL, the grade and what is left in issues.
