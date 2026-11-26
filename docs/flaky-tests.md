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
  -f specs=tests/rendering/far-build.play.spec.ts:66 -f projects=webgpu,browser-defaults -f repeat=30
```

Each project runs on its own runner. The job summary lists, per test, the passes, the
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
