# Agent runtimes

Four coding-agent runtimes share one judge, `aoe-harness agent-hook --runtime
<runtime> <event>` (`crates/harness/src/agents/`), through the launcher
`.agents/hooks/harness.sh`. Instructions stay canonical in
[AGENTS.md](../AGENTS.md) (imported by [CLAUDE.md](../CLAUDE.md)); every runtime
reads them. Hooks are the only enforcement: no runtime layer carries its own
permission rules, and per-machine files (`.claude/settings.local.json`,
`CLAUDE.local.md`) stay out of Git. This extends the
[provider-neutral harness](adr/0006-provider-neutral-harness.md); it adds no
second quality policy.

| Runtime | Committed layer | Hook wiring | Roles |
|---|---|---|---|
| Claude Code | `.claude/settings.json`, `.claude/agents/`, `.claude/skills/` | Native hooks | `agent_type` of subagents, or `AOE_AGENT_ROLE` |
| Codex | `.codex/hooks.json`, `.codex/agents/*.toml` | Native hooks; matcher `Bash`/`apply_patch` (the patch text names its files) | `AOE_AGENT_ROLE` only: Codex reports no subagent type |
| DeepSeek Harness (dsh) | `.dsh/hooks.json` | The `@deepseek-ai/dsh-hooks-claude-code` bridge plugin | `AOE_AGENT_ROLE` only: the bridge reports `general-purpose` |
| pi | `.pi/extensions/aoe-harness/index.ts` | Extension: `tool_call` blocks, `agent_end` forces a follow-up | `AOE_AGENT_ROLE` only |

**Roles across runtimes.** The harness starts each role it needs (tester,
implementer, reviewer) as its own headless process with `AOE_AGENT_ROLE` set
(`crates/harness/src/agents/launch.rs`). Hook processes inherit it and an agent
cannot change it, so the role holds on every runtime. An in-process subagent of
Codex, dsh or pi that the harness did not launch is judged as the main session.

## Setting up each runtime

- **Claude Code:** nothing to do; `.claude/settings.json` is committed.
- **Codex:** trust the project once (`/hooks` review). Codex reads `.codex/` from
  the **main repository root**, not from a linked worktree, so interactive
  sessions in a worktree are judged only once the main checkout has `.codex/`.
  Harness-launched Codex runs pass `.codex/hooks.json` inline
  (`-c hooks.<Event>=…`) and are judged in any checkout.
- **DeepSeek Harness:** the bridge reads one absolute config per process. Add to
  `$DSH_HOME/cordis.patch.yml` (or pass with `--patch`):

  ```yaml
  - insert:
      - name: '@deepseek-ai/dsh-hooks-claude-code'
        config:
          configPath: /absolute/path/to/AoeWorld/.dsh/hooks.json
  ```
- **pi:** project extensions load once the project is trusted (`pi -a` for one
  run). pi needs a model login (`/login`).

## Live verification

`make agent-smoke RUNTIME=<runtime>` runs the runtime's own CLI headless as a
launched `implementer` and asks for three commands: host `cargo` (refused), a
write into the protected `gates/` (refused) and a write into `.cache/tmp/`
(allowed). It reads the file system, not the model's account, and writes
`reports/agents/<runtime>-smoke.json`. Run it from a clean, committed tree: the
stop rule judges the agent's whole working tree.

| Runtime (version) | Last result | Evidence |
|---|---|---|
| Claude Code | PASS | Both refusals in `permission_denials`; allowed write done |
| Codex 0.159.0 | PASS | Hook denials quoted; protected write absent; allowed write done |
| dsh 0.2.0-rc.2 | Bridge verified; tool-call run needs credentials | `SessionStart` reached the hook with the expected payload. Headless runs need a model credential in the launching shell (`AOE_DSH_PROVIDER`/`AOE_DSH_MODEL` select an authenticated provider) |
| pi 0.85.1 | Not yet run live | No model login on the verification machine; adapter covered by unit tests |

## The judge

**Transition.** A branch created before this adapter carries the older
`.claude/hooks/harness.sh`, which looks for `crates/harness/src/claude/` on
`origin/dev`. Once that path is gone it builds a bootstrap judge from its own
checkout (labelled non-authoritative) until it rebases onto `dev`, so rebase open
branches after this adapter merges. Rebasing also moves the hook environment
from `AOE_CLAUDE_HOOK_*` to `AOE_AGENT_HOOK_*` and the records from
`.cache/claude-hook/` to `.cache/agent-hook/`.

`harness.sh` runs a cached `aoe-harness` binary built in the pinned
`aoeworld/rust-tools` image by `make agent-hook-build`, under the Git common
directory (`aoe-agent-hook/`, shared by worktrees, never committed). The judge
is built from `refs/remotes/origin/dev`, so a branch cannot change the rules that
judge it; a new `dev` commit triggers one rebuild. While `origin/dev` has no
runtime adapter yet, the judge is built from the checkout and labelled a
**non-authoritative bootstrap** at session start. The registry and limits are
always read from the checkout.

The first build takes a few minutes. If it fails (Docker down), PreToolUse fails
closed: every shell and edit call is refused until a person runs
`.agents/hooks/harness.sh build` in a terminal. Other events only report.

## Events

| Event | Does | Timeout |
|---|---|---|
| SessionStart | Context: checkout, branch, HEAD, merge base, judge, role, Git-hook status, last check-fast, `BLOCKED.md`, progress saved before compaction, the rules | 900 s |
| PreToolUse (shell and file-edit tools of each runtime) | Judges the call by role; a denial names what to do instead | 900 s (first build) |
| PostToolUse (file-edit tools) | Edit cadence on the one file: UTF-8, 500 lines, 14 code/config files per directory | 60 s |
| Stop, SubagentStop (pi: `agent_end`) | check-fast and the red-round rule | 1800 s |
| PreCompact | Saves `git status`, the last 8 commits and a red report for SessionStart | 60 s |

The hook reads at most 4 MiB of JSON and always exits 0. Unreadable, oversized or
mismatched PreToolUse input is denied.

## Roles

A role comes from `AOE_AGENT_ROLE` (set by the harness when it launches an
agent) and from Claude Code's `agent_type`, so each file in `.claude/agents/` is
named after its role. When both are set, the narrower role applies; an empty or
`main` value restricts nothing, and tester plus implementer together may write
nothing.

| Role | `agent_type` | May write | Git and GitHub |
|---|---|---|---|
| Main | absent | Everything except `.git/` and `.cache/agent-hook/` | Commit; publish with `make ship` |
| Tester | `tester` | Tests only: `tests.rs`, `tests/`, `*_tests.rs`, `*.spec.ts`, fixtures, `fuzz/` | Read only |
| Implementer | `implementer`, `coder` | Code, never a test | Read only |
| Reviewer | `reviewer`, `verifier`, `qa`, `Explore`, `Plan`, `claude-code-guide`, `reviewer-*`, `rv-*` | Nothing | Read only |
| Other | any other agent | Outside the protected classes | Commit; publish with `make ship` |

Every agent may also write temp directories, `.cache/tmp/` and
`.cache/agent-hook/BLOCKED.md`.

**Protected classes** are edited only by the main session, where a person is
present: harness policy (`crates/harness/`, `.agents/`, `.claude/`, `.codex/`,
`.dsh/`, `.pi/`, `make/`, `CLAUDE.md`, every `AGENTS.md`, `Makefile`, `docker/`,
`skills/`, `docs/adr/`, this page, the root Cargo, toolchain, deny and ignore
files), gates (`gates/`, `.github/`) and `baselines/`. `.github/CODEOWNERS` routes the same paths to a person.

## The Bash policy

The line is flattened into simple commands: pipes, lists, subshells (with their
own working directory), redirections, here-documents and `$(…)`/backtick/process
substitutions, each judged as a line of its own. `sh -c`, `env`, `timeout`,
`nice`, `nohup`, `exec`, `command`, `watch`, `xargs`, `find -exec` and
`rebase --exec` hand their command on to be judged in turn, up to 8 deep. `cd`
and `pushd` are followed; after a `cd` the policy cannot follow, relative paths
are refused to agents. Write targets are resolved through `~`, `..` and symlinks
(dangling ones too) and compared case-insensitively.

Denied to **every role**:

- `sudo` and other privilege tools;
- host `cargo`, `rustc`, `rustfmt`, `npm`, `npx`, `wasm-bindgen` and similar:
  the denial names the Dockerized Make target;
- `--no-verify` in any spelling (`-n` on commit, abbreviations), `git -c`,
  `--config-env`, `--git-dir`/`--work-tree`, and git configuration that runs
  commands or loads files (aliases, pagers, editors, `core.hooksPath`,
  `include.*`, filters, textconv, credential helpers, `push.default`);
- `HOME`, `XDG_CONFIG_HOME`, `GH_CONFIG_DIR`, `MAKEFLAGS`, `GIT_CONFIG*`,
  `GIT_DIR` and other variables that reconfigure Git, gh or Make;
- `make -f/-e/-i/-t/-o/-W/--eval`, and overriding `SHELL`, `*_IMAGE`, `*_RUN`,
  `*_MOUNTS` or `GIT_*` on the Make command line;
- any `git push` or `send-pack` and `gh pr create`: work is published only by
  `make ship` ([shipping](shipping.md)); `update-ref` on `dev`, `main` or
  `release/*`, and force moving them (`branch -f`, `checkout -B`, `switch -C`);
- `gh pr merge`, approving a review, `gh pr update-branch`, repository,
  ruleset, secret, release and auth changes, gh aliases and extensions, and
  `gh api` writes to merges, refs, contents, branches, reviews, checks or
  GraphQL mutations;
- any write into `.git/`, the Git common directory or `.cache/agent-hook/`.

Denied to **agents** in addition: interpreters (`python`, `node`, `perl`…),
scripts or binaries run by path, `eval`, `source`, shells without `-c`,
computed command names and write targets, network tools and non-localhost
`curl`/`wget`, Docker except read-only subcommands, links, patches,
`git apply`/`am`, git configuration writes, code-running environment variables
(`PAGER`, `EDITOR`, `GIT_SSH_COMMAND`, `LD_PRELOAD`, `PATH`…) unless set to a
no-op, sed `w`/`e` and awk redirection or `system()`, writers behind `xargs` or
`find -exec`, whole-tree unpacking or recursive copies into the checkout, Make
targets that belong to a person (`hooks-install`, `bootstrap`, releases) and
Make variables other than `AOE_*`, `HARNESS_*`, `REVIEW_*`. Every other command
must be on a short read-mostly allow-list. Tester, Implementer and Reviewer
only read Git and GitHub; the Reviewer runs checks, not `make fmt`.

## The stop rule

On Stop and SubagentStop the hook runs **check-fast**: the gates of
`gates/registry.json` with the `stop` cadence and `static: true` that the
change selects (working tree and untracked files against the merge base with
`$HARNESS_BASE`, default `origin/dev`; a missing base selects every suite).
The Tester runs only those that are also `edit` cadence, since its tests fail
on purpose; the Reviewer runs nothing. Gates run as written in the registry
(`make fmt-check`, `make lint`…) with their budgets; Docker gates are
UNAVAILABLE when `docker info` fails, which makes the run INCOMPLETE, never PASS.

A fingerprint of HEAD, the merge base, `git diff HEAD --binary` and untracked
contents caches the result, so stopping again without a change reuses it.
State lives in `.cache/agent-hook/state.json` under a file lock.

- **Agents** are blocked with the report for 5 red rounds (counted per
  `session_id/agent_id`), then told to write `.cache/agent-hook/BLOCKED.md`
  with what they tried, what fails and what they need. A non-empty
  `BLOCKED.md` newer than the streak lets them stop. Green clears the streak; a
  full green deletes `BLOCKED.md`. INCOMPLETE counts no round.
- **The main session** is blocked once per red change so it sees the report,
  then told and let go: a person is present.

check-fast is feedback, not the handoff gate: `make pre-commit`, `make
preflight` and CI remain mandatory, and build-wasm, unit tests and the rest of
the stop cadence run there.

## Threat model and limits

Against subagents every rule must hold, and an escape is a bug. Against the main
session the rules catch mistakes and shortcuts; a construction built on purpose
to defeat them is a documented limit, backstopped by the Git hooks, CI and the
person who merges. As in [ADR 0006](adr/0006-provider-neutral-harness.md), none
of this is a hostile same-user boundary:

- The policy reads shell text and the files commands name, never the code an
  interpreter or a Make target runs. Agents may not run interpreters; the main
  session may.
- Role identity is `AOE_AGENT_ROLE` and Claude Code's `agent_type`, not an
  authenticated launcher identity. In-process subagents of Codex, dsh and pi are
  judged as the main session unless the harness launched them. Rust inline `#[cfg(test)]` modules inside production files are not
  separated by file ownership; keep tests in `tests.rs`/`tests/` modules.
- Hidden (sealed) tests, the test-first workflow script and canaries for each
  rule are not built yet. `make ship` (per-commit evidence) and its review gate
  are described in [shipping](shipping.md) and [review](review.md).
- Read, Grep and Glob are not intercepted: nothing in the repository is secret
  from an agent.
