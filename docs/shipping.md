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

`aoe-harness ship` (`crates/harness/src/ship/`), run on the host:

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
7. **Creates or updates** the pull request against `dev` with the title and
   description. A new branch needs both `SHIP_TITLE` and `SHIP_BODY`.

CI then runs every selected gate again; the `required` check gates the merge,
and GitHub auto-merge merges an armed pull request when the required checks
pass. No agent runs a merge command.

## Limits

- `make ship` is local feedback with recorded evidence; it is not a protected
  judge. A same-user process can still forge files it can write, and CI is the
  authoritative re-run.
- The preflight gates run twice per ship (once by `ship`, once by the pre-push
  hook). The duplicate keeps hook bytes unchanged; removing it needs a migration
  of installed hooks.
