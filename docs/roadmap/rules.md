# Roadmap: standing rules in force

| rule | since | source |
| --- | --- | --- |
| Quiet machine while the owner plays: no builds, no agents; never touch the served checkout; never leave a test match | 11-09, 11-15 | owner "I want the machine free when I play" |
| Perf is judged by instructions per tick on `make save-replay`, never wall time on a loaded machine; a perf brief names every CI-gated counter (late_game, advance, smoke_ticks, distributed_ticks, viewport_query) | 11-12, 11-14 | owner "Can't you measure instruction count?" |
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
