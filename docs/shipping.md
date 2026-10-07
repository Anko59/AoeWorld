# Shipping

Every agent runtime (Claude Code, Codex, DeepSeek Harness, pi) publishes work
the same way, and only this way:

```sh
make ship SHIP_TITLE='One-line title' SHIP_BODY=/path/to/description.md
```

The agent policy denies `git push` and `gh pr create` to every role (see
[agent runtimes](agent-runtimes.md)). Tester, implementer and reviewer agents do
not ship; the session that ran them reviews, commits and ships.

## What `make ship` does

`aoe-harness ship` (`crates/harness/src/ship/`), run on the host by the same
judge binary as the agent hooks (`.agents/hooks/harness.sh exec ship`, built
from `origin/dev`; while bootstrapping, from the checkout's committed HEAD and
labelled non-authoritative), never from uncommitted edits. `SHIP_TITLE`, `SHIP_BODY` and
`SHIP_FORCE` reach it through the environment, never through shell text, and the
agent policy refuses `$` in agent-set Make values.

0. **Checks first** that `gh` is 2.100 or newer (PR video uploads) and, when
   the branch has no open pull request, that a title and a description file
   were given. Nothing runs or is pushed otherwise.
1. **Refuses** a tree with uncommitted or untracked changes ("evidence is for a
   commit"), the protected branches `dev`, `main` and `release/*`, and a
   detached HEAD.
2. **Selects** suites from the change against `refs/remotes/origin/dev` after a
   fetch, never a local branch or tag that could shadow it.
3. **Runs** the registry's `preflight` gates at that exact commit, as written in
   `gates/registry.json`, with their budgets. Docker gates are UNAVAILABLE when
   Docker is down, which makes the run INCOMPLETE, never PASS.
4. **Checks** that HEAD, the tree and the branch did not move while the gates
   ran; otherwise it writes no evidence.
5. **Writes evidence** whatever the verdict, keyed by the full commit id:
   `<git common dir>/aoe-ship/evidence/<sha>.json` (`make ship-status` prints it).
   A new commit has no evidence, so evidence for one commit never stands in for
   another. The agent policy refuses every write into the Git common directory.
6. **Only on PASS**, pushes exactly that commit to `origin` as the branch
   (`SHIP_FORCE=1` adds `--force-with-lease`, after a rebase). The pre-push hook
   runs `make preflight` again.
7. **Creates or updates** the pull request against `dev` on `origin`'s GitHub
   repository, found by head branch (never by a number).

CI then runs every selected gate again; the `required` check gates the merge,
and GitHub auto-merge merges an armed pull request when the required checks
pass. No agent runs a merge command.

## Limits

- `ship` runs the `preflight` cadence (recorded as `cadence` in the evidence),
  not every `pr`-cadence gate: browser, WASM, coverage, fuzz and target
  performance gates run in CI, whose `required` check gates the merge.
- Gates run in the checkout as written (`make <gate>`), not in an exported
  commit snapshot: ignored files and caches take part, and an edit reverted
  between the before and after checks is not seen.
- `gh` is checked for a minimum version, not pinned to an image yet.
- `make ship` is local feedback with recorded evidence; it is not a protected
  judge. A same-user process can still forge files it can write, and CI is the
  authoritative re-run.
- The preflight gates run twice per ship (once by `ship`, once by the pre-push
  hook). The duplicate keeps hook bytes unchanged; removing it needs a migration
  of installed hooks.
