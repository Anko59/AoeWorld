# AoeWorld for Claude Code

@AGENTS.md

The rules above are canonical; this file only adds what is specific to Claude
Code. [Agent runtimes](docs/agent-runtimes.md) explains every hook and how
Codex, DeepSeek Harness and pi get the same rules.

## What the hooks do

`.claude/settings.json` sends every hook event to `aoe-harness agent-hook`
through `.agents/hooks/harness.sh`. A denial always says what to do instead:
follow it rather than rephrasing the command to get past it.

- **Every Bash, Edit and Write call is judged** by the caller's role. Host
  `cargo`/`npm` are refused (use the Dockerized Make targets), as are hook
  bypasses (`--no-verify`, `git -c`, hook or alias configuration), edits to
  `.git/` or `.cache/agent-hook/`, and merging, approving or pushing
  `dev`/`main`/`release/*`.
- **Edited files** are checked at once against the 500-line and 14-files rules.
- **Stopping** runs check-fast: the static stop-cadence gates that
  `gates/registry.json` selects for the change. Red results come back to you;
  fix them without weakening a test, gate or baseline.

## Shipping

Commit on a feature branch (the pre-commit hook runs `make pre-commit`), then
publish with `make ship SHIP_TITLE=… SHIP_BODY=<description file> SHIP_TIER=<tier>`:
it runs the preflight gates at that exact commit, records evidence, runs the
tiered [adversarial review](docs/review.md) (choose the tier; `make
review-floor` prints the minimum), and only at 8/10 or more pushes, opens the
pull request against `dev` and arms auto-merge ([shipping](docs/shipping.md)). Direct `git push` and
`gh pr create` are refused. GitHub merges when the required checks pass; never
merge, approve, or push `dev` or `main`.

## Credentials

API keys and tokens are in the keyring, not in the environment: before saying
you lack a credential, look it up with
`secret-tool lookup service codex-api name <VARIABLE>` and set it only for the
command that needs it ([credentials](docs/credentials.md)). Never print it.

## Roles

Delegate test-first work to the agents in `.claude/agents/`: `tester` writes
failing tests only, `implementer` writes production code only, `reviewer` only
reads. They never commit or push; the main session reviews, commits and ships.
Built-in read-only agents (`Explore`, `Plan`) are held to the reviewer's rules.
Any other agent may work outside the protected classes (harness, gates,
baselines, `.claude/`, instructions), which only the main session edits.

## Threat model

Against subagents every rule must hold; an escape is a bug. Against the main
session the rules catch mistakes and shortcuts, while a construction built on
purpose to defeat them is a documented limit backstopped by the Git hooks, the
adversarial review and CI: `dev` merges only when both the `required` CI check
and the `harness/review` status pass, and no session merges. The policy reads shell text and the files commands name,
never the code an interpreter runs.
