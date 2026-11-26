# Flaky browser tests

A browser test is fixed when it passes many times in a row on a GitHub runner, not
when it passed once on a laptop: a busy Mac aborts page loads with
`net::ERR_NETWORK_CHANGED` (see [local development](local-development.md)), so a local
run proves nothing about flakiness.

## Prove a fix: `e2e-repeat`

`make test-e2e-repeat` repeats chosen specs on chosen Playwright projects and tallies
passes against failures per test. The logic is `crates/harness/src/e2e/repeat`; the
workflow `.github/workflows/e2e-repeat.yml` only runs it on a runner. It never uses the
quarantine and never retries: every attempt counts as it ends, with one game server and
one worker, as in a CI shard.

| Variable | Meaning |
| --- | --- |
| `AOE_E2E_REPEAT_SPECS` | spec files under `browser/tests`, `tests/<path>.spec.ts`, optionally with `:line` to run one test |
| `AOE_E2E_REPEAT_PROJECTS` | Playwright projects (`webgpu`, `browser-defaults`, `canvas`, `firefox-webgl2`, `firefox-webgpu`) |
| `AOE_E2E_REPEAT` | attempts per test, 1 to 200, default 30 |

A workflow file can be dispatched only once it is on `dev`, and runs the branch you
name with `--ref`:

```sh
gh workflow run e2e-repeat.yml --ref <branch> \
  -f specs=tests/rendering/far-build.play.spec.ts:66 \
  -f 'projects=["webgpu","browser-defaults"]' -f repeat=30
```

Each project of the JSON array runs on its own runner. The job summary lists, per test, the passes, the
failures, the passes that close the run and one character per attempt in start order
(`.` passed, `F` failed, `T` timed out, `I` interrupted, `S` skipped), then the first
error line of each failure. The artifact `e2e-repeat-<project>` holds `repeat.json` (the
same tally), `repeat.md`, the Playwright report and the traces of failed attempts. The
job fails when any attempt fails, when a test has fewer attempts than asked, when a test
never ran (all attempts skipped) or when no test ran (the spec does not run on that
project). The run is cut off after 90 minutes and then has no tally, because Playwright
writes its report at the end: split a long repeat into several dispatches.

`make test-e2e-repeat` is refused to agents like the other whole suites: dispatch the
workflow instead.

## Count flakiness

A browser test that fails and then passes on one immediate retry does not fail the pull
request, but it is never silent. The rule, in `crates/harness/src/e2e/quarantine`:

- After a completed failure of a test (an assertion in an `expect` step; never a setup
  or fixture failure, a worker exit or a test timeout), the harness reruns exactly that
  test once, on its own project. Playwright's own retry count stays zero.
- More than three failed tests in one run (timeouts counted) are a broken change, not a
  flake: nothing unlisted is retried.
- It passes the rerun: the job passes, and the test is recorded as flaky (below).
- It fails the rerun: the job fails. A test that fails twice fails the job.
- A test in `gates/flaky.json` keeps its two reruns and names its issue in the record; the
  quarantine stays the explicit, expiring exception.
- `AOE_E2E_SPECS` runs and `make test-e2e-repeat` follow the same code: a filtered local
  run retries once; the repeat run never retries. Duel checks never retry.

The record: one JSON line per flaky test in `reports/e2e/<report>.flaky.jsonl` (test,
project, first error, attempts, shard, commit, run), a table in the job summary, and the
CI artifact `e2e-flaky-<shard>` (kept 14 days, only when a shard flaked).

The nightly triage (`make nightly-triage`, job `triage`) reads the `e2e-flaky-*`
artifacts of the last seven days. A test flagged in three artifacts (three shard runs)
gets an issue titled `Flaky e2e test: <test> [<project>]`; a later night comments on
the open issue instead of opening another. Close it once `e2e-repeat` shows 30 passes.
