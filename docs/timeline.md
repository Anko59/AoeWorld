# Delivery timeline

`make timeline` reports the last seven days across the repository's worktrees.
Set `TIMELINE_DAYS=14` for a different window. It reads local records and calls
GitHub through the host's `gh` login; it never publishes anything. The target
builds the checkout's harness in Docker so a new command can be tried before
it reaches the judge on dev.

`make ship` appends stage boundaries to
`<git common dir>/aoe-ship/timeline/<branch>.jsonl` (branch slashes create
subdirectories). Each line has `t` (Unix seconds, fractional), `branch`, `head`,
`stage`, `event` (`start` or `end`) and a `detail` object. `detail.run` separates
processes; end details include `ok` or the outcome. Journal failures never
change the command's result. Abruptly killed stages can have only a start.
Within `review`, each `review-round` has its own boundaries, tier, round number,
`kind` (`blind` or `cross`) and closing flag. Rounds have no individual grade;
the final grade, once known, is on the enclosing review's end. Recheck gates
record attempts and overload-retry load waits just like the main gates.

The report pairs boundaries by branch, process, stage, gate, tier and round,
including across a rebase, and counts only completed pairs. Repeated starts replace an
unfinished start; missing boundaries and malformed records are reported.
Durations are attributed to the completion window; they are not clipped at its
start. Median averages the middle two samples; p90 uses nearest rank.

Gate evidence and review JSON supply earlier history, including failed runs.
Journal observations take precedence over overlapping history for the same
head and stage. Reused review reports do not count as new review time.
Historical gate durations count recorded retry attempts with each attempt's own
verdict when available.
Historical files retain only their latest run for a head/tier: overwritten
attempts and historical lock and push timings cannot be recovered.

Merged PRs are joined by branch and final head; earlier local attempts on that
branch appear in time order. An observed successful final push supplies
push-to-merge time. Without it, the report labels created-to-merge as a proxy,
never as a measured push. CI shows all returned runs for the final head,
including failures, with elapsed time from `startedAt` to `updatedAt` (GitHub's
last update, approximately completion), not summed job CPU time. Parallel CI
and local stages overlap; substage and enclosing totals must not be added.
The top five rank stage totals, including breakdowns; these overlap.
Historical per-gate rows show stored gate order and execution durations;
individual start times were not recorded.
For PRs merged in the window, `ship-to-merge` starts at their first retained
completed ship attempt. The ship clock starts in Rust, after judge bootstrap.
PR joins search retained local history before applying the merge-time window,
so a final push before the window still supplies the full push-to-merge interval.
Local stage aggregates and CI aggregates use their own completion times. CI
queries do not exclude runs created before the window.

GitHub query failures leave a useful local report and an explicit warning.
Queries are bounded; the report warns when their limit is reached. Branch
reuse, missing local records, old overwritten evidence, CI reruns later than
merge and GitHub's retention limits constrain historical completeness. No
record is proof of a gate passing or of performance qualification.
