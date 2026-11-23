# Roadmap: standing rules in force

| rule | since | source |
| --- | --- | --- |
| Quiet machine while the owner plays: no builds, no agents; never touch the served checkout; never leave a test match | 11-09, 11-15 | owner "I want the machine free when I play" |
| Perf is judged by instructions per tick and by wall time, wall time only on a quiet machine (no other build or match running, load average under 3 before the run; median of 3 where repeated, load written beside it; otherwise instructions only, and say so); a perf brief names every CI-gated counter (late_game, advance, smoke_ticks, distributed_ticks, viewport_query) | 11-12, 11-14, 11-21 | owner "Can't you measure instruction count?"; #978 rule 3 |
| Replay-hash equality is not required; track and justify behaviour changes; gameplay acceptance is `make quality-metrics` | 11-13 | owner "don't be afraid to break things" |
| Prove the measurement before calling a metric a gameplay defect | 11-14 | the attack-move harness bug |
| Visible changes ship with in-game before/after video; invisible ones say no video; never cards-only; no recordings in Git | 10-28, 11-03 | owner |
| Owner builds: `owner-build.sh`, DE pack with building overlay verified, match token set, played by me first, my tab closed, errors logged | 10-15, 10-30, 11-10 | owner |
| Movement tests use real generated terrain (rivers, forests, bridges, 100-unit marches), never open ground only | 10-29, 11-02 | owner |
| Only Definitive Edition assets | 10-29 | owner |
| No backward compatibility before v1.0 (snapshot layout changes still bump the version) | 10-10 | owner, AGENTS.md |
| Ambition and rule for the rewrite: no queue (units never silently wait), level of detail allowed | 11-07 | owner |
| Check an item against `git log origin/dev` before briefing it; never mark DONE without a merged PR | 11-14, 11-16 | owner "forget earlier decisions" |
| Say explicitly which reported items are in a build and which are not | 10-28 | owner |
| One worktree per task off origin/dev; `git -C` with absolute paths; clean worktrees after merge; keep 50 GB free | 11-07 | owner |
| Never lower a gate or bypass hooks; change the harness instead of designing around it | 10-10 | owner |
| Read the CI perf comment on every PR | 10-28 | owner |
| Kill a branch after two failed attempts; ship or cut rewrite branches within hours; bigger PRs | 11-11, 11-12 | owner |
| Codex first for implementation; Sonnet or Haiku subagents otherwise; at most 4 Codex sessions | 11-09, 10-28 | owner |
| Never end a turn with nothing running when the owner gave the machine | 11-05 | owner |
| Scale work follows the steps of #978 in order (its State comment holds the current step and the frozen references); judge by growth, not by percent: work is worth doing when it stops a cost from growing with units, and a saving that does not and is under 5% at 61k is listed and skipped; every speed number comes with the service numbers; one line a day on #978 | 11-21 | owner, #978 (replaces the #865 queue of 11-16) |
| Solve, not cap: never add a cap or budget as a fix; existing caps are removed per [caps](caps.md), each expensive one together with the fix of its cost | 11-23 | owner "We need to solve, not limit and hide" |
| Experiment lane: `exp/...` branches with hooks on but no review, video or docs; two days to a first number; never merged as is; only a winner is redone as ordinary PRs | 11-16 | issue #865 rule 4, hooks kept on |
