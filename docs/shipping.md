# Shipping

Every agent runtime (Claude Code, Codex, DeepSeek Harness, pi) publishes work
the same way, and only this way:

```sh
make ship SHIP_TITLE='One-line title' SHIP_BODY=/path/to/description.md
```

The agent policy denies `git push` and `gh pr create` to every role (see
[agent runtimes](agent-runtimes.md)). Tester, implementer and reviewer agents do
not ship; the duel driver commits each step and the main session reviews and ships.

## What `make ship` does

`aoe-harness ship` (`crates/harness/src/ship/`), run on the host by the same
judge binary as the agent hooks (`.agents/hooks/harness.sh exec ship`, built
from `origin/dev`; while bootstrapping, from the checkout's committed HEAD and
labelled non-authoritative), never from uncommitted edits. `SHIP_TITLE`, `SHIP_BODY`,
`SHIP_FORCE` and `SHIP_BASE` reach it through the environment, never through shell text, and the
agent policy refuses `$` in agent-set Make values.

0. **Checks first** that `gh` is 2.100 or newer (PR video uploads) and, when
   the branch has no open pull request, that a title and a description file
   were given. Review-ahead needs `SHIP_BODY` for the task, but no title yet.
   Nothing runs or is pushed if these checks fail.
1. **Refuses** a tree with uncommitted or untracked changes ("evidence is for a
   commit"), the protected branches `dev`, `main` and `release/*`, and a
   detached HEAD.
2. **Selects** suites from the change against the fetched
   `refs/remotes/origin/<base>` (`dev`, or a published `SHIP_BASE`). For an
   unpublished stacked parent it explicitly selects `refs/heads/<parent>`.
   The chosen commit is pinned for gates and review; tags cannot shadow it
   (see [stacked pull requests](#stacked-pull-requests)).
3. **Runs** the registry's `preflight` gates at that exact commit, as written in
   `gates/registry.json`, with their budgets. Docker gates are UNAVAILABLE when
   Docker is down, which makes the run INCOMPLETE, never PASS.
4. **Checks** that HEAD, the tree and the branch did not move while the gates
   ran; otherwise it writes no evidence.
5. **Writes evidence** whatever the verdict, keyed by the full commit id:
   `<git common dir>/aoe-ship/evidence/<sha>.json` (`make ship-status` prints it).
   A new commit has no evidence, so evidence for one commit never stands in for
   another. The agent policy refuses every write into the Git common directory.
6. **Reviews** the commit with the tiered [adversarial review](review.md)
   (`SHIP_TIER`, never below `make review-floor`; `SHIP_RUNTIME` picks the model
   family). A full review below 8/10, an incomplete review, or a closing review
   with a blocking finding or an undecided carried finding pushes nothing.
   A branch gets one full blind review; a non-severe failure leads to up to
   three closing reviews of the fixes and carried findings. Severe findings
   or unresolved carried blockers end the budget early; when it is spent
   `make ship` says to split the change
   ([review](review.md#when-reviews-do-not-converge)). Commits added after a
   passing review are reviewed on their own diff, never by a new review of the
   approved code ([review](review.md#commits-after-an-approval)). A stored passing review
   may also be reused after a rebase when the branch's own patch matches,
   even after edits to its parent. The harness compares byte-exact paths,
   modes and changed text lines, ignoring context, offsets and hunk grouping;
   binary changes and gitlinks require both original object IDs. It preserves
   whitespace and final newlines ([review](review.md#what-happens-next)). The source diff uses
   its recorded base commit; the target uses the selected base now. The old
   review must meet the required tier, belong to the same branch and task,
   and match current `origin/dev` review policy. A complete report for the
   current head against this base blocks copying another review over it.
   A new report points to the original passing full or closing review;
   reuse reports never become sources. Gates run before loading any approval
   on every ship, and CI runs them again. Status and description say
   `review of <sha> reused (same patch)`.
   Reuse after a rebase for `make review-pr` is planned in issue #146
   ([review](review.md#pull-requests-the-harness-did-not-open)).
7. **Only then**, if the parent is ready, pushes exactly that commit to
   `origin` as the branch (`SHIP_FORCE=1` adds `--force-with-lease`, after a rebase). The pre-push hook
   runs `make pre-push`: it skips the preflight when
   `make ship`'s passing preflight evidence covers the clean HEAD being pushed
   (`AOE_SHIP_EVIDENCE`, checked in Rust), and runs `make preflight` otherwise. The push sets SSH keepalives (unless
   `GIT_SSH_COMMAND` or `GIT_SSH` is set) because the hook can idle the
   connection for minutes (#209).
8. **Creates or updates** the pull request against the base (`dev`) on
   `origin`'s GitHub repository, found by head branch (never by a number),
   with the review appended; posts the `harness/review` status and, into
   `dev` only, arms auto-merge.
9. **Files the review's leftovers** as issues: one per confirmed or disputed
   critical or major finding, and one `Review follow-ups for #<pr>` checklist
   for the minor and nit ones, deduplicated across ships
   ([issues](issues.md#review-follow-ups)). A follow-up that cannot be filed is
   reported and does not fail the ship.

## One ship at a time

Sessions share one machine, so `make ship` and `make duel` take a machine-wide
lock before any heavy work and release it when they exit, by whatever route.
It is an OS file lock on `<git common dir>/aoe-ship/ship.lock`, shared by every
worktree and checkout of the repository; a dead or killed holder frees it, so
it is never stale. The file also names the holder (worktree, branch, command,
start time), for messages only.

Waiters are served in arrival order. Each creates a ticket in
`aoe-ship/ship.lock.queue/` (named by arrival time, pid and a counter) and keeps
an OS lock on it while it waits and while it runs; it may take the main lock only
when no older ticket is still locked. A ticket whose lock can be taken belongs to
a dead process: the next waiter removes it and moves on. Tickets are removed on
release and on drop, so none are left after a normal exit.

A second run prints `ship: waiting for <branch> in <worktree> (started <time>)`
and `ship: position N in the queue`, then a line every 5 minutes (and whenever
its position changes), and gives up with an error after
`SHIP_LOCK_WAIT_MINUTES` (default 120; `0` fails immediately). `make ship-status`
shows the holder and the queue in order without waiting. Read-only commands (`review-floor`,
`showcase-check`, `ship-status`…) never take the lock. Never wait for another
ship by scanning processes: a `pgrep -f` loop matches its own command line and
waits on itself forever; queue on the lock instead.

## Stacked pull requests

A change that depends on an unmerged pull request does not wait for it:

```sh
make ship SHIP_BASE=<parent branch> SHIP_TITLE=… SHIP_BODY=…
```

The base may be `dev`, an open harness pull request's branch on origin, or a
local parent branch whose pull request does not exist yet. Another protected
branch, the child itself and an unknown branch are refused. A closed or merged
parent pull request requires a rebase onto `origin/dev` instead; a foreign or
manually opened parent pull request is refused as before.

**Review ahead:** when the parent has no pull request, the local parent tip
must be an ancestor of the child. The harness fetches `origin/dev` for review
policy, pins the local parent commit, runs the gates on the child's current
tree and reviews only `parent..child`. Evidence and the stored report record
the parent branch and commit. It stops with
`reviewed ahead: <grade>; push when the parent <branch> has its pull request`:
no push, child pull request, status or auto-merge is published. `SHIP_BODY`
is still the review task; a PR title can wait. `make review SHIP_BASE=<parent>
REVIEW_TASK=<same description file>` uses the local parent too, runs the gates
before reviewing or loading an approval, stores the report and never publishes.

Once the parent has its open harness pull request, run `make ship` with the
same base and description. Suites, review, test-first and metrics use the
merge base with the fetched `refs/remotes/origin/<parent>`. Gates run again;
a stored approval is reused if the child's own patch and the other guards
still match, even if the parent was edited and the child rebased. The child
PR opens with `--base <parent>` and says it is stacked on #<parent>. CI runs
on pull requests into any branch, so both are tested at once.
`harness/review` is posted, but auto-merge waits for a restack onto `dev`.

After the parent squash-merges, rebase onto `origin/dev` (`git rebase --onto
origin/dev <old parent tip>`) and run `make ship SHIP_BASE=dev`. A matching
own patch reuses the stacked review
([review](review.md#what-happens-next)). The pull request is retargeted to
`dev` (unless GitHub already did), force-pushed with lease when the pushed
branch is no longer an ancestor of HEAD and its last review was against
another base, and auto-merge is armed. A changed own patch needs another
review under the branch's remaining budget. Moving an open pull request from
`dev` onto a parent disables previously armed auto-merge.

## The pull request description

`SHIP_BODY` follows `.github/pull_request_template.md`: a level, `## Why` (one
grounding element, such as a quote from the human request, an issue or reference
link, an HTTPS URL, a screenshot or a sampled metric, plus at most two lines), `## What` (at most two lines) and
`## For AI`. `make ship` refuses a description that breaks those limits ("split
the PR") and renders two parts:

- **🧑 For humans:** Why, What with the showcase video (`SHIP_VIDEO`, uploaded
  inline with `gh --attach`), and How with the review headline, summary, and a
  metrics table comparing the merge base with this PR ([metrics](#metrics)).
- **🤖 For AI:** the agent's notes, the [test-first report](#test-first), gate
  evidence and the full adversarial review.

| Level | For | Video (`SHIP_VIDEO`) | Voice | Target share |
|---|---|---|---|---|
| low | the smallest PRs (≤ 60 changed lines) | none | none | 20 % |
| medium | most PRs | ≤ 1 min | none | 30 % |
| high | important PRs | ≤ 2 min | required | 40 % |
| max | the most critical PRs | ≤ 5 min | required | 10 % |

`gates/levels.json` holds these limits (video length, voice-over, the `Why` and
`What` line limits, the metrics row set: `summary`, `standard`, `extended` or `full`, which picks the rows of the [metrics table](#metrics)), the target shares and the
floor rules. Like `gates/review.json` it is read from origin/dev, so a branch
cannot loosen its own limits (the working tree only while dev lacks the file).

### Checking the description first

`make describe-check SHIP_BODY=<file> [SHIP_TITLE=…] [SHIP_VIDEO=<file>]` runs
every description rule at once (every role; read-only, no lock, no fetch): the
template sections and their line limits, the `Why` grounding element, the level
marker, the level against the floor (with its reason), the video against the
level (required or not, length, voice-over, unreadable file) and the title. It
prints `description: OK (level <level>, floor <floor>)` or a numbered list of
every problem. `make ship` runs the same check before the lock wait, the gates
and the review, and stops with the whole list.

### The floor

The agent picks the level, but never below the floor `make ship` computes from
the change: `make level-floor` prints it (every role; read-only) and the rule
that demands it. A rule names a level, a minimum of changed lines (added plus
removed against the merge base; a binary file counts as 61 in all, not per side) and optionally the
registry suites or path prefixes the change must touch (a `behaviour` rule
counts only files that are neither tests nor documentation or configuration); the floor is the
highest level of any matching rule. Today:

- more than 60 changed lines: at least `medium`;
- any product suite selected (native, gameplay, browser, assets, performance,
  map, release, everything) by a file that can change behaviour: at least
  `medium`. Test files (the `is_test` classification of the roles) and the
  documentation and configuration paths of `gates/levels.json` (`docs/`,
  `skills/`, `make/`, `.github/`, `gates/`, `AGENTS.md`, `CLAUDE.md`, `*.md`)
  select nothing here, so a test-only change, a docs-only change or a
  make/gates/workflow edit of at most 60 lines may be `low`. The floor is
  about the description only: the gates and the review still run in full on
  those files. Harness code, `.claude/` and `.agents/` still count as
  behaviour;
- 200 or more changed lines touching the harness, gates or instructions
  (`everything`), the map or geodata crates (`map`) or `crates/protocol/`: at
  least `high`. Tests and docs count towards this size rule.

Shipping below the floor is refused before any gate runs, with the reason and
the level to use. The chosen level, the floor, its reason and the metrics row
set of the level are recorded in the ship
evidence (`level`) and shown in the "For AI" part, which also carries the
`<!-- level: … -->` marker.

### Tracking

`make level-stats` (main session; read-only, uses `gh`) reads the marker of
the last 50 pull requests merged into `dev` (`stats_window`) and prints their
distribution against the target; pull requests without a marker are counted
apart. `make ship` adds one line with the same distribution to the "For AI"
part; a `gh` failure there only says the line is unavailable.

The agent picks the level. Not being a frontend change is never a reason for no
video: film a terminal or agent session, a before/after timing or an API diff.
Record it with `make showcase` ([showcase videos](showcase.md)). Duration and
audio-stream presence are checked with ffmpeg from the pinned browser image. High and max levels require an audio stream; reviewers decide
whether that track contains speech.

## Metrics

`make metrics` writes the metrics of one commit (HEAD, or `METRICS_REV=<rev>`)
to `<git common dir>/aoe-ship/metrics/<sha>.json` and prints it. Every role may
run it: it only reads. The PR table is built from the snapshots of the merge
base and of HEAD, as base → PR with a delta and a 🟢/🔴 marker where a metric
has a good direction. It has one row per metric, and the PR level (the first line of
`SHIP_BODY`) picks the rows:

| Level | Rows |
|---|---|
| low | lines changed by class, `#[test]` attributes (unit, integration), production comment density and lines, largest human-authored file |
| medium | + generated lines changed, wasm, Playwright and fuzz test counts, Cargo and npm dependencies, `unsafe` lines, TODO/FIXME lines, files within 10% of or over the 500-line limit |
| high | + total lines and comment density per class, gate time, WASM size (raw, gzip), native server binary |
| max | + line coverage and the perf smoke figures (p95 tick, entities, encoded bytes) |

**Static** values come from git blobs at that commit and need no build. Lines
use the classes production, tests, harness, docs, config and generated
(lockfiles, `docs/gates.md`). Comment density uses the Rust lexer for `.rs` and
whole `//` or `#` lines for TypeScript, shell, YAML and TOML. Test counts: Rust
`#[test]` in unit files (`crates/*/src`) and integration files
(`crates/*/tests`), `#[wasm_bindgen_test]`, Playwright `test(` cases in
`browser/tests`, and fuzz targets (`fuzz/fuzz_targets/*.rs`). Dependencies are
the third-party packages (with a `source`) of `Cargo.lock` and `fuzz/Cargo.lock`
and the packages of `browser/package-lock.json`. `unsafe` counts non-comment
production lines with the keyword; TODO/FIXME counts production, test and
config lines. The largest-file and limit rows use the same human-authored
text files as `make structure-check`. Blobs over 1 MiB (16 MiB for lockfiles)
and binary blobs are omitted.

**Measured** values are never produced by `make metrics`: it reads reports that
already exist and shows "—" otherwise. They apply to the checked-out commit
only and only when the report was written after that commit was made: coverage
(`reports/coverage/summary.json`), WASM size (`reports/perf/optimized.wasm`,
`.gz`, else the largest `web/pkg/*.wasm`), the native binary
(`target/release/aoe-server`), the perf smoke report (`reports/perf/smoke.json`,
clean and for that commit) and the gate seconds of that commit's ship evidence.
A later snapshot of the same commit keeps measured values an earlier one had.
Measured rows with no value on either side are left out of the table.

The comment scanner ignores markers inside ordinary and raw strings and handles
nested block comments; it does not parse macro-generated tokens or languages
embedded inside strings.

CI then runs every selected gate again; the `required` check gates the merge,
and GitHub auto-merge merges an armed pull request when the required checks
pass. No agent runs a merge command.

## Tester and implementer: make duel

Product work starts with the main session's driver on a clean feature branch:

```sh
make duel DUEL_TASK=.cache/task.md DUEL_RUNTIME=codex
```

The task is a regular UTF-8 file at most 64 KiB; its first line titles the
commits (at most 72 characters, shortened at a word boundary with “…”). The full
task stays in both commit bodies. Keep it outside tracked changes (for example under `.cache/`).
`DUEL_RUNTIME` supports `claude`, `codex` and `pi`; dsh is not supported yet.
When present, `CLAUDECODE`, `CODEX_THREAD_ID`/`CODEX_CI` or `PI_CODING_AGENT`
identify the calling runtime; absent or ambiguous markers require an explicit choice. `DUEL_MODEL`
and `DUEL_EFFORT` select a model and reasoning effort; absent values use the
runtime's configured defaults. pi accepts `provider/model`.

The driver launches a tester with the project hooks and `AOE_AGENT_ROLE=tester`,
checks every changed path is a test outside protected classes and refuses
deleted or renamed existing Rust test functions (body edits remain reviewable), then runs the
check red. Only a failed build or a failing test belonging to a test file the
tester added or changed that round leads to a `Tests: <task first line>` commit,
with `Harness-Phase: tests`. The implementer receives the task, tester report
and failing output tail, writes production only, and gets three attempts to
turn the check green. The driver commits that implementation with
`Harness-Phase: implementation`. Commits use the configured Git identity and
name the runtime/model in a `Co-Authored-By` trailer, with `noreply@anthropic.com`
for Claude, `noreply@openai.com` for Codex and neutral `noreply@example.com` for pi. Before staging a commit,
the driver runs `make hooks-check`; failure stops with `run make hooks-install`.
Each duel commit runs the
Git pre-commit hook (`make pre-commit`); preflight and the wider shipping gates
run in `make ship`, while both role prompts require only the focused check.

A package compile error reported by Cargo is red because a test may name
production APIs not written yet. A missing nextest command, invalid arguments,
or another failure without compile errors or failing-test events makes the
check unavailable and refuses the step without committing. The tester's test files
are then kept uncommitted and the driver records them (branch, HEAD and a content hash
of each path) in `<git common dir>/aoe-ship/duel-kept/<branch>.json`. A rerun resumes
from them only when the dirty tree matches that record exactly; any other dirty tree is
refused (`commit first` / `commit or stash`) and left untouched. The record is removed
when the round commits or the duel ends otherwise. A resumed round never runs
`git reset --hard` or `git clean` over that work: a passing red check, an unauthored red
or a role violation stops with the reason and leaves the files in place.
If tests cannot compile for a reason other than missing production code (such
as a syntax error in a test), the implementer stops and reports
`DUEL_TEST_BUILD_ERROR: <reason>` on its own line. The driver ends the duel with
that message immediately, keeping the tests commit and working changes.

`DUEL_ROUNDS` defaults to 2 and accepts 1–4 total rounds. Later tester runs
attack the implementation; no new test or a passing attack ends successfully
and passing attack edits are discarded. A default check with only unrelated failing
tests is treated like passing tests, and the summary names those failures.
Round one's empty or passing tests fail. Role violations restore to the last
driver commit and name offending paths. Implementers may change production lines
in a file containing inline tests, but cannot change any line in an item under
a `cfg` mentioning `test` outside `not(...)`, or any part of a `#[test]`-style function, including
its attributes and body. Malformed product token streams skip inline validation;
the focused compiler check reports the failure and the implementer can retry. Testers
still edit test files only. Exhausted green attempts keep the tests
commit and working changes for the main session to finish. A launch timeout is
a failure, never red evidence.

The default command is `make duel-check HARNESS_DUEL_BASE=<round-start commit>`.
It runs through Docker and selects only Cargo packages owning tester-changed
files (`cargo nextest run -p <package>`) and exact changed browser spec files.
Browser fixtures under `browser/tests/` select all browser specs. Mixed changes
run both. A changed file of a package that `make test-wasm` runs (the suites in
`wasm_test::SUITES`, today `aoe-client` and `aoe-rendering`) selects that package's
wasm-bindgen suite in the Docker browser stage instead of native nextest, which finds
no tests there: a file with `wasm_bindgen_test`s (or no tests, a helper) runs wasm, one
with `#[test]`s runs nextest, one with both runs both. The wasm results are named
`<package>$<module path>::<test>` and follow the same red/green/authorship rules, and a
wasm compile failure of the test target is a build-failed red. A wasm run that exits
non-zero with neither a named failing test nor a compile error (a crash, a timeout, a
lock-file error) makes the check unavailable, never red; rerun it or use `DUEL_CHECK`. Fuzz targets in their separate workspace and other unsupported paths
stop before the implementer launches and require `DUEL_CHECK` (for example
`make fuzz-smoke`). Red means a selected package fails to build or at least
one failing test belongs to a tester-changed file, matched by nextest binary id
or module path, or browser test location; green means every selected check passes
and every name that failed at red is reported as passed. After a red build
failure, every test defined in the test files the tester changed that round must
run and pass (at least one). Missing or skipped tests refuse green with
`the tester's tests did not run: <names/files>`; this also blocks the pre-existing
failure exemption below.
Integration failures follow the target's declared module path to the defining
file, which must have changed that round. Unresolved sources fall back to a
changed integration target; the summary records that fallback.
When the check cannot run (a missing tool, a timeout, a runner that never started) the
tester's test files are kept, uncommitted, in the working tree, and the duel ends
saying so and how to rerun with `DUEL_CHECK=`. A rerun on a tree whose only changes are
test files continues from them: round 1 skips the tester and goes straight to the red
check. Any change outside test files still refuses the start.
`DUEL_CHECK='make <focused target>'` overrides this trusted main-session command.
For an explicit override, a normal nonzero exit is red and exit zero is green;
the red-test execution requirement does not apply. Named failures are reported when available but are not required, including
for fuzz runtime crashes. The main session chooses this check and its scope.
Both role prompts name the exact check and forbid preflight or whole-suite runs;
the driver and `make ship` own wider gates. Browser dependencies are prepared on
the first browser check and reused for subsequent checks and baseline exports
during that duel. Each check retains nextest and Playwright JSON in a fresh
random directory under `.cache/duel/run-*`, with exclusive file creation that
refuses existing files and symlinks. The check prints the retained paths.
Retained nextest and Playwright JSON are limited to 16 MiB
total and 1 MiB per line; exceeding either makes the check unavailable.
Each agent and check gets `DUEL_BUDGET_MINUTES` (default 60); the process group
is killed on expiry.

A default focused green failure is compared with the original pre-duel revision
when all failed tests are demonstrably untouched. If those same tests fail there, the driver
records `pre-existing failure: <names>` and commits without relaunching the
implementer. Unknown/build failures or changed tests cannot receive this exemption.
The focused check still reports FAIL; the summary records the exception.

`DUEL_QUALIFY=<command>` (for example `make preflight` or `make test-e2e`) adds a
final qualification step once the last round is green: the driver runs the command
once in the worktree, under the same process runner and budget handling as the
checks (`DUEL_QUALIFY_MINUTES`, default 45). A pass is recorded as
`qualify: PASS (<command>, <duration>)` and the result line then says what is left
(read the diff, film, ship). A failure relaunches the implementer with the failure
excerpt and the instruction to make the command pass without touching any test; the
same role rules apply (test files and inline test code must stay byte-identical, the
focused check must still pass). The fix is committed as its own commit
(`Harness-Phase: qualification`) and the command is rerun, at most
`DUEL_QUALIFY_ATTEMPTS` times (default 2); then the duel ends FAIL with
`qualification failed: <excerpt>` and every earlier commit stays. An implementer
that reports `DUEL_QUALIFY_BLOCKED: <reason>` (the failure is in a test it may not
change) stops the duel with that reason. Without `DUEL_QUALIFY` nothing changes.

The summary includes the full task, runtime/model, each round's red/green results
with the failing test names (a green line shows a count such as `266 passed`, never
a list of passing tests; with a `DUEL_CHECK` override the names come from the
check's output, and `unidentified` only means nothing was recognisable), commits,
output tails (the failure lines, at most 20) and each agent's last report. It appears
on stdout and in `.cache/duel/<branch>.md`, replacing branch slashes with `_`;
the driver prints that path last, including on failure. The main session then
reads the diff, films the change with `make showcase` and runs `make ship`. `dev`
can move during a duel: read the change with the three-dot diff
`git diff origin/dev...HEAD` (a plain `git diff origin/dev` also shows what landed on
`dev` meanwhile), or rebase first.
`duel` and all `DUEL_*` variables are main-session only. Protected harness work
follows the same tests-first order in the main session because these roles
cannot edit the harness itself. This is local workflow evidence, not a sealed
CI proof that a particular assertion failed for the task's intended reason.
Coverage is measured by `make coverage`; runtime launches in harness tests are
stubs, so use a real task separately to verify a live runtime.

## Test-first

A pre-commit that stages only test files runs clippy without test targets, so
red tests that name code not written yet can be committed first; the next
commit and every preflight check all targets.

Test-first is reported, never enforced: `make ship` proceeds whatever the
report says. From Git alone (merge base..HEAD), `aoe-harness`
(`crates/harness/src/ship/test_first/`) writes a short report into the
description's "For AI" part and into the facts every reviewer reads:

- each commit classified as tests-only, product, mixed or other, where product
  code is any non-test file under `crates/` except `crates/harness/`; changed
  lines that all lie inside `#[cfg(test)]` items of a product file count as
  test changes (Rust token and item spans), and such files are listed, since
  the rule is tests in test files; a merge is listed as `merge (<kind>)` from
  its diff against its first parent;
- the number of `#[test]`-style functions (`#[test]`, `#[tokio::test]`,
  `#[rstest]`…, never a `cfg`) added before the first product commit;
- build-time files changed: `build.rs` or a custom `package.build` script,
  `.cargo/`, `rust-toolchain*`, `.config/nextest.toml`, and a manifest's
  `build`, `links`, build dependencies (target tables with their selector) or
  profiles, and the `[workspace.dependencies]` entries that build dependencies
  inherit, parsed with `toml`;
- an exemption trailer, `Harness-Test-First: exempt — <reason>`, if any.

A change to the harness, gates, docs or CI alone, with no build-time file, is
"not applicable". The
test-integrity reviewer judges the report: product code with no failing test
before it, a build-time change or a weak exemption is a finding, not a refusal.
Proving in CI that the tests ran red then green remains
[#167](https://github.com/Anko59/AoeWorld/issues/167).

## Limits

- `ship` runs the `preflight` cadence (recorded as `cadence` in the evidence),
  not every `pr`-cadence gate: browser, WASM, fuzz and target performance
  gates run in CI, whose `required` check gates the merge. Coverage is not
  a `pr` gate: it takes 18 to 34 minutes, so it runs on every push to `dev`
  or `main` and nightly, where a drop is seen within one merge.
- Gates run in the checkout as written (`make <gate>`), not in an exported
  commit snapshot: ignored files and caches take part, and an edit reverted
  between the before and after checks is not seen.
- `gh` is checked for a minimum version, not pinned to an image yet.
- `make ship` is local feedback with recorded evidence; it is not a protected
  judge. A same-user process can still forge files it can write, and CI is the
  authoritative re-run.
- The pre-push hook trusts `make ship`'s local evidence for the pushed commit;
  like all local evidence it is not a protected judge, and CI re-runs every
  gate. A manual push without that evidence runs the full preflight.
