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
mesh against the remote integration branch. Draft
[PR #41](https://github.com/Anko59/AoeWorld/pull/41) reviews the parser fuzz
targets at clean revision `5d50d0dc9cc669e78511eb68bfcfabbd4a0c0734`;
all seven targets passed 512 smoke runs each. Schema-9 seeds and independent
review remain open. [PR #42](https://github.com/Anko59/AoeWorld/pull/42)
reviews typed hydrology, schema compatibility, and integration fixes at
`647531b079da01fd12ffa9b51fc3f012bc1825aa`. Its independent Luna review
approved the typed branch. Add links to further focused PRs as they are opened.
Reports from dirty
worktrees or older revisions do not count as final integrated evidence.

## Integrated typed evidence checkpoint

The merge at `647531b079da01fd12ffa9b51fc3f012bc1825aa` combines typed
hydrology branch `6105f94abbed30fdf656db2dc9044322b3a8de9c` with the
existing worker-progress and scratch-recovery paths. It publishes separate
bounded hydrology and modern land-cover pages, writes schema 9 and compact
chunk v2, and retains schema-8 and compact-v1 decoding. Four direct-vector
terrain builders reject typed packages without their evidence provider.

At this clean merge, `make hooks-install`, `make hooks-check`, `make fmt-check`,
`make map-test` (82/82), `make test-unit` (425/425), and `make preflight`
(425/425 plus WASM build and performance smoke) passed. The pre-commit and
pre-push hooks passed. A genuine older source-backed 100 km schema-8 package
still passed `AOE_MAP_PACKAGE=... make map-verify` with unchanged hash
`bd78ad64118ad3fc40d3681dc8ebcff39c857a09c41185b2eb80e6d4e81b23a4`.
The separate schema-8 terrain fingerprint fixture covers fallback generation;
it is not a prepared-page generation fixture.

## Fixed 100 km qualification in progress

The uncommitted recipe-4 source branch generated and verified the same fixed
Paris request at hash
`f591d9b8c347c2d268dfcf4baaa8c97a8390ffb8c41c857b5dc402fee26302b3`.
An earlier staged version briefly used a 3×3 start footprint only for recipe 4.
That policy change was rejected during integration: recipes 3 and 4 both keep
the established 5×5 clear start footprint. The fixed east endpoint
`(49998,24999)` is passable grass with no
resource or water, but exact sparse A* proves it disconnected from the chosen
start in 4,137 work units and 461 peak retained entries, far below the
12,800,000/524,288 caps. All 23 eligible starts found within the same 64
chunks lie in six fully enumerated components of 308–458 tiles; none reaches
the east endpoint. The west fixed endpoint and predetermined nearby edge
probes also remain disconnected. This is currently a source-backed traversal
failure, not a performance qualification. The branch is diagnosing the
component boundary before any resource-policy change.

The separate rendering follow-up is committed at
`68d3f4f208b370c9fc989e789ab6c344d03c0d4d`, with synthetic E2E 29/29,
WASM, and preflight passing on its branch. Private-pack render/select/move/
reload checks passed, but the full private suite timed out at its 180-second
deadline during case 29. Captures showed diagnostic grass without loaded map
chunks, so source-backed relief and water visuals remain unqualified. An
independent review requested fixes for diamond UVs, initial high-relief
visibility, selected-ring occlusion parity, and shallow-water tint parity.
