# Shipping a change

Read [shipping](../../docs/shipping.md) and [review](../../docs/review.md).

Product work starts with `make duel DUEL_TASK=<file> DUEL_RUNTIME=<runtime>`
(or the same tests-first order). The driver commits each red-test and green-code
step; read its summary and diff, then film the change before shipping.

1. Commit on a feature branch; run `make review-floor` and `make level-floor`
   and pick the tier and the description level (never below them; higher for
   risky changes). Write the description file and run
   `make describe-check SHIP_BODY=<file>` first: it lists every problem at once.
2. `make ship SHIP_TITLE=… SHIP_BODY=<file> SHIP_TIER=<tier> SHIP_RUNTIME=<your runtime>`.
   A change can be reviewed ahead of a local parent with `SHIP_BASE=<its branch>`;
   gates and review run, but publication waits for the parent's harness pull
   request. Once it exists, ship stacked on it (no auto-merge yet); after it merges,
   rebase onto `origin/dev` and ship again with `SHIP_BASE=dev`
   ([stacked pull requests](../../docs/shipping.md#stacked-pull-requests)).
3. Full review below 8/10: fix and commit every confirmed finding (never weaken a
   test or gate), file out-of-scope ones as issues, ship again.
4. A branch gets one full blind review. If it fails without a critical finding
   or major test weakening, `make ship` runs up to three **closing reviews** of
   the fixes since the last reviewed commit and every carried blocking finding
   (commit the fixes first); new findings must be in that fix diff. Closing
   reviews pass when no blocking finding or undecided carried finding remains
   and every session answered, regardless of grade. A
   closing review with severe findings or unresolved carried blockers, or three
   failed closing reviews means **split** the change into smaller pull requests:
   do it, close the original, file leftovers as issues. Never rerun reviews to fish for a
   grade, lower the tier, or edit reviewer prompts or `gates/review.json` in
   the same branch.
5. Report the pull request URL, the grade and what is left in issues.
