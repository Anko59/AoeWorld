# CI report page

`make ci-report` builds one offline HTML page of the CI's numbers over time:
`reports/ci/index.html` (open it in a browser) and `reports/ci/data.json` (the
data it draws). `reports/` is not committed. The page loads nothing from the
network, follows the light or dark theme and reads at 400 px wide; its charts
are inline SVG and a few lines of inline script show the values under the
pointer. The logic is the `ci_report` module of `aoe-harness`; the Make target
only runs it.

```sh
make ci-report                          # the first run lists all history, later ones only what is new
make ci-report AOE_GH_ACCOUNT=Anko59    # on the Mac, pick the gh account that can read the repository
```

On a Mac the target runs in the OrbStack machine like every other goal
([local development](local-development.md)); `gh` and `ssh` there run the Mac's
own programs. The command also tries `git fetch origin dev` and warns, rather
than fails, when that does not work.

## What the page shows

1. One sentence on the latest measured `dev` commit: how many gated rows fail
   or warn, the late-game count and the bundle against its limit.
2. Instruction counts on `dev` over time, one chart per family (late-game rows,
   small-world rows `*_ticks`, single operations), and the compressed client
   bundle against its baseline and limit. A row whose baseline in
   `baselines/perf/micro.json` changed by more than two times between two
   commits was redefined: its line restarts at that commit, which carries a
   lettered dashed marker, so a redefinition never reads as a regression. The
   late-game chart therefore shows the change since the last redefinition (point
   at it for the raw counts). An accepted increase (`accepted_increase` in
   `micro.json`) gets a dotted marker.
3. CI time: the median duration of each job per UTC day, the longest job of a
   run, and machine-minutes per run (every job's time added up).
4. Where each gated row stands against its baseline and limit, with the gate's
   own thresholds (`perf_micro`: warn above +1%, fail above +3%, +10% for the
   total of a kernel that also reports its main thread or for creep over the
   fixed reference, a new baseline due below -3%; the bundle fails above +25%).
5. Every move of 2% or more of a row between two measured commits, with the
   commit and its pull request; a move across a redefinition is marked.
6. Reliability, from the GitHub Actions API: per job, the share of `ci.yml`
   runs on pushes to `dev` whose final attempt failed, per week, the runs where
   the job failed first and the run passed on a rerun, and the step that failed
   most often. It covers the last eight weeks of runs.

## Where the numbers come from

- The performance comment ([performance](performance.md)) that CI posts on each
  pull request starts with `<!-- aoe-perf-comment -->`. Its `dev` column and the
  `dev` commit it names give one measurement of `dev`; its `CI job | Duration`
  table gives the time of each job of that run. The command lists the
  repository's issue comments and keeps those of `github-actions[bot]`.
  CI rewrites a comment on every run, so every version seen is a separate
  run; versions are kept in `.cache/ci-report/comments.jsonl` and the cursor in
  `comments.cursor`, so a second run asks GitHub only for comments edited
  since. Delete `.cache/ci-report/` to start over.
- Commit dates and titles come from `git log origin/dev`; baselines, their
  redefinitions and accepted increases from the Git history of
  `baselines/perf/micro.json` and `wasm.json`; thresholds from the constants of
  `crates/harness/src/perf_micro.rs` and `perf.rs`.
- Actions runs and jobs come from `gh api`; runs that completed are cached in
  `.cache/ci-report/actions.json` and fetched again only after a rerun.

The parser reads the comment's tables as `perf_comment` renders them. A change
to that format must keep the fixtures in `crates/harness/src/ci_report/tests/fixtures`
(real comments, old and new wording) parsing; the tests fail otherwise.

## Limits

- A `dev` commit that no pull request was compared with has no point.
- Timing rows (tick times) are left out: CI prints them only outside a noise band
  on shared runners.
- CI time covers pull request runs (one per comment version), not runs of `dev`
  itself; reliability covers `dev` runs only.
- Only the 2x baseline rule detects a redefinition. A workload that changed by
  less than two times shows as an ordinary move.
