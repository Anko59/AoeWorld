# Geographic map continuation, 22 September 2026

This is the working ledger for the continuation from integration commit
`403d98d017492d5c45ed1649d1957cf17ee5f814` on
`codex/map-completion-integration`. The checkout was clean at handoff and three
commits ahead of its recorded remote branch. The plan and acceptance criteria
remain in [completion.md](completion.md); this ledger records active work and
revision-specific evidence.

## Preserved starting work

The tracked diff and untracked source files from each dirty worktree were saved
outside Git at
`/home/anko/Work/projects/AoeWorld-continuation-backup-2026-09-22/` before
new edits. The worktrees themselves remain intact:

| Worktree suffix | Branch or state | Starting commit | Owner | Next acceptance |
| --- | --- | --- | --- | --- |
| `map-typed-hydrology` | `codex/map-typed-hydrology`, dirty | `0ca2cd174ef397b20fe5538441bf384988942a7f` | Luna xhigh geodata | Genuine schema-8 fixture, schema-9 typed pages, integrity and residency tests |
| `map-schema8-fixture` | detached, dirty | `0ca2cd174ef397b20fe5538441bf384988942a7f` | Luna xhigh geodata | Generate old bytes and expected hash using the old implementation |
| `map-source-qualification` | `codex/map-source-qualification`, dirty | `76154946d012579efbee922d0bb6ddab6c1d0db3` | Luna xhigh elevation | Cell-centred interpolation, recipe migration, unchanged 100 km activation |
| `map-qualification` | `codex/map-parser-fuzz`, dirty | `5470212cdf39f634193d1d842c07e7db4d690847` | Orchestrator | Integrate parser targets and seed typed pages after package migration |
| `map-surface-mesh` | `codex/map-surface-mesh-integration`, clean | `2fdfa8b0b7613ae3f4e86ddc6934000f3b9ae803` | Luna xhigh rendering | Independent mesh review, then textured terrain and backend parity |

The older `map-geodata` worktree at `04464752609a4bcd74a7d4d4c5943f20233dac65`
is retained as an old baseline, not an integration base.

## Known failures and limits at handoff

- The saved typed patch's nominal schema-8 fixture has generator version 9 and
  does not prove compatibility with genuine older output.
- The saved interpolation patch maps samples as edge-aligned; source samples are
  cell-centred. A fixed 100 km Paris-region source package previously failed
  ordinary activation with artificial cliff edges near the start.
- Native line coverage was 15,848 / 19,589, below the 85% policy floor.
- Browser performance evidence was inconclusive. Synthetic routing tests do not
  qualify a source-backed 100 km journey.
- Historical area allocation, correction policy, coherent water surfaces,
  textured ramps/cliffs, and final source-backed lifecycle remain unfinished.

## Pull requests and evidence

The pre-existing stack through [PR #38](https://github.com/Anko59/AoeWorld/pull/38)
remains open. Its commits are mostly present locally; do not replay the whole
stack. [PR #39](https://github.com/Anko59/AoeWorld/pull/39) reviews the already
implemented submission recovery against the preparation-progress branch.
[PR #40](https://github.com/Anko59/AoeWorld/pull/40) reviews the shared terrain
mesh against the remote integration branch. Add links to further focused PRs
here as they are opened. Reports from dirty
worktrees or older revisions do not count as final integrated evidence.
