# Geographic map continuation checkpoint: 24 September 2026

## Starting state

Work resumed from integration revision
`dc85302947d93024c8008659b26d9971583ee7e1` on
`codex/map-completion-integration`. The tree contained a preserved staged
recipe-5/elevation/source-qualification continuation plus focused unstaged
fixes. Before further edits, tracked patches, status, HEAD, and untracked source
or configuration files were copied to
`/home/anko/Work/backups/AoeWorld-preserved-worktrees-20260924`. Private assets,
downloaded geodata, generated packages, and caches remain outside Git and
outside that source backup.

## Parallel work

Three implementation workers use the configured model alias
`mimo-v2.6-flash` in isolated worktrees:

| Worker | Worktree | Owned deliverable |
| --- | --- | --- |
| Mencius | `/home/anko/Work/projects/AoeWorld-map-hyde-area-allocation` | Equal-area, area-conserving HYDE allocation and focused evidence |
| Locke | `/home/anko/Work/projects/AoeWorld-map-qualification` | Geographic parser fuzzing, schema-9/legacy seeds, and smoke evidence |
| Socrates | `/home/anko/Work/projects/AoeWorld-map-surface-mesh` | Textured shared-surface rendering, cliffs, water, occlusion, LOD, and backend parity |

The orchestrator retains package/version integration, source qualification,
review, and final gates. Worker output is not final evidence until reviewed and
integrated at one clean revision.

## Integration decisions

- Cell-centred rational elevation interpolation uses deterministic symmetric
  nearest rounding, with exact halves away from zero.
- Generation recipe 5 changes source-elevation sampling identity and uses the
  `maps-v8` package directory. Missing recipe fields remain recipe 3 and keep
  legacy serialization and identity behavior.
- Recipes 3 through 5 require the established 5 by 5 clear start footprint.
  The recipe argument validates supported generation behavior; it does not
  weaken gameplay policy for better-looking qualification results.
- Source-qualification component diagnostics use candidates satisfying that
  5 by 5 footprint and the 256-tile reachability requirement. The 3 by 3
  metrics remain comparison diagnostics only.
- Schema-9 typed hydrology and modern land-cover pages are persisted,
  verified, resident-bounded, and visible to terrain. Schema-8 packages remain
  valid when those optional pages are absent.

## Source-backed qualification evidence contract

The source qualification runner accepts only a recipe-5 package with verified
source locks, one-to-one compression, and the fixed 50,000-tile/100 km request.
Its JSON report keeps three independent top-level sections:

- `logical_memory` records indexed and walked pages, bounded residency, resource
  overlay changes, and separate route, replay, and combined navigation-cache
  peaks. Navigation-cache bytes are logical payload accounting only and exclude
  allocator and map container overhead; they are neither an RSS cap nor a dense
  map-allocation claim.
- `process_rss_bytes` records separately sampled process `VmRSS` start, peak,
  and end values from `/proc/self/status`. Peak is the maximum of explicit
  samples, not the kernel lifetime high-water mark; a missing valid `VmRSS` is
  `null` rather than zero.
- `simulation_work` records route and replay legs, moved distance, ticks,
  simulated seconds, periodic checkpoints, exact hash/state comparisons, and
  the resource-lifecycle evidence. Synthetic work remains insufficient to
  establish finished RTS performance or dedicated-hardware qualification.

Lifecycle evidence covers three real loopback `/game/ws` connections: the
initial controller, a network disconnect and ResumeToken reconnect that must
receive the revision-1 full snapshot, and a fresh controller after a full
service stop. The replacement recreates `PageResidency` from package pages and
`GameplayService::from_stored_map` from the persisted resource overlay, and must
report a changed `world_id()` with an identical `resource_snapshot()` before the
fresh client reconnects.

The report also carries these explicit source-workload contracts:

| Tiles per side | Contract | Dataset claim |
| ---: | --- | --- |
| 512 | Bounded sparse package/request test | No source dataset available, not claimed |
| 16,384 | Bounded sparse package/request test | No source dataset available, not claimed |
| 50,000 | Fixed 100 km source-backed qualification runner | Requires verified source package locks |
| 262,144 | Sparse maximum package/request contract | No source dataset available, not claimed |

The bounded cases prove request bounds and absence of dense fine-tile
allocation only. They do not substitute for unavailable real datasets or a
completed source-backed run at those axes.

## Acceptance next

The next integrated checkpoint must demonstrate, without changing the fixed
seed, Paris request, route endpoints, or search bounds:

1. Recipe-3 identity and recipe-5 interpolation/identity tests, including
   dense/provider parity, shared corners, page transitions, missing pages, and
   cancellation.
2. Verification and ordinary activation of the fixed recipe-5 package, with
   any start or route failure classified by actual cause.
3. More than 128 verified pages walked through one bounded residency provider,
   followed by deterministic reload of an evicted page.
4. The fixed 100 km movement/replay workload and source-backed resource
   depletion across eviction, reload, restart, and client reconnect.
5. Reviewed HYDE allocation, water reconstruction, rendering, parser fuzz,
   browser/WASM/E2E, coverage, and performance-policy results at the final
   revision.

## Current limits

- The previous fixed-package route result used the rejected 3 by 3 diagnostic
  policy and is superseded; the unchanged source-backed workload remains
  unqualified until rerun with recipe 5 and 5 by 5 starts.
- Historical area allocation, coherent water reconstruction, and textured
  surface completion are still in worker slices.
- Native line coverage was previously 81.6%, below the unchanged 85% floor.
  Browser performance remains inconclusive.
- Source-backed 100 km travel/replay and resource reconnect qualification have
  not yet been demonstrated at the integrated revision.
- Synthetic workloads do not demonstrate finished RTS performance or qualify
  dedicated hardware.
