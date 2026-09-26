# Geographic maps: continuation after the final-session checkpoint

This checkpoint was requested by the user to stop spending quota and transfer
implementation to another agent. **The feature is not ready to merge.** Stop
instructions superseded the remaining implementation and qualification work.
This document supersedes the remaining-work section of
[the earlier handover](handover-2026-09-26.md), which retains the older PR inventory.

## Start here

- Canonical combined branch: `codex/map-final-integration`,
  [draft PR #72](https://github.com/Anko59/AoeWorld/pull/72).
  Use its latest pushed head, not its old `88feef3` checkpoint.
- Local combined checkout: `/home/anko/Work/projects/AoeWorld-final`.
  The default `/home/anko/Work/projects/AoeWorld` checkout is older.
- The preservation commit containing this document contains all intended
  combined code, including the unfinished, unwired lake rasterizer.
- Read `AGENTS.md`, `docs/agent-engineering.md`, and scoped instructions before
  changes. Build/check through Dockerized Make; never bypass hooks or lower
  acceptance limits. Human-authored files: at most 500 lines; directories:
  at most 14 directly contained code/config files.
- User requested **gpt-6-luna, xhigh** subagents for straightforward work.
  Keep at most three workers plus an accountable orchestrator, use isolated
  workspaces or explicit non-overlapping ownership, and avoid duplicate audits.
- First fix recipe-6 activation, finish the lake rasterizer, then qualify the
  combined source paths. Do not launch expensive final fuzz campaigns before
  the parser/code revision is stable.

## Product contract and scope

Select a geographic square, physical size, and compression ratio; generate
AoE2-style terrain/resources approximating that location around 600 CE.
Use WGS84 local azimuthal-equidistant projection for maps and equal-area
allocation for historical quantities. Distinguish source observations,
historical models, explicit corrections, procedural detail, and fallback data.
Modern natural water is evidence, not proof of its exact year-600 extent.

Tiles represent two game meters. Supported requested extents are 250 m through
10,000 km; compression 1 through 10,000; virtual dimensions 64 through 262,144
tiles per side. The maximum is a sparse storage/sampling contract, not proof of
finished large-scale RTS performance. Economy, buildings, combat, and naval
units are outside this feature. Do not change navigation caps, start clearance,
fixed source cases, or resource distribution just to make qualification pass.

## What is integrated

The combined tree reconciles the earlier fuzz, geographic navigation, source
visuals, water, vegetation, historical correction/streaming, creator, and
rendering branches. Do not cherry-pick all old PRs again.

Content integrated includes `da65404`, `193b95c`, `5e5b9c4`, `f1b25e1`,
`32e1248`, `2fddffa`, and historical serialization commit
`90a1385b15c6f3ad4fcc9326d30e2e46a6bec9cb`.
Water follow-up `b31561b388552ce891deeec67425f4f2ca2c47a2` was committed in
`AoeWorld-map-water-corrections`; its contents were integrated via patch,
including its final sampler guard fixes. The four-file historical reader
follow-up in `AoeWorld-historical-corrections` remains locally uncommitted but
is fully copied into the combined checkpoint. Commit ancestry alone therefore
does not establish whether a follow-up is already integrated.

### Historical reconstruction and ordinary workers

- Ordinary overview and detailed worker inputs carry historical, vegetation,
  and water correction documents through the production entry points.
- Independent historical grids reach `min(detailed axis, 1024)`; overview
  normally uses 128. Compact coverage serialization fits page byte limits and
  retains legacy decoding. Correction documents and preprocessing identities
  participate in source/package identity.
- HYDE source quantities are opened as Float64 through restored thread-local
  `AAIGRID_DATATYPE`, avoiding GDAL's default Float32 precision loss.
- Canonical 4320×2160 HYDE grids use exact five-minute boundaries, repairing
  the rounded ASCII header's date-line gap. Large allocations subdivide before
  existing source-window and densified-vertex limits; output remains 64×64 pages.
- Preserve HYDE's spherical valid-land denominator; distribute extensive
  quantities using WGS84 equal-area overlap fractions. Source capacity uses
  a 6371 km sphere with 0.0001 km² source rounding tolerance. Crop/grazing
  arithmetic allows only four f64 EPSILONs of binary rounding, not Float32 loss.
- Historical preprocessing is now `hyde-600ad-area-pages-v3`. Consecutive
  duplicate vertices introduced by projection/densification are removed;
  duplicate original polygon vertices remain rejected.
- One genuine inconsistent source cell was observed at HYDE row 1670,
  column 1281 (roughly 49.17°S, 73.25°W): nonzero crop with zero valid land.
  It remains an explicit error. Do not silently reinterpret it as valid zero.

### Water and rendering

- Water model v2 retains bounded HydroRIVERS reach IDs, downstream IDs, and
  distance-to-sink metadata. Supported edges follow source topology and have
  non-increasing modeled heights; unsupported direction remains unknown.
- Lake/correction junction anchors are preserved. A cited `SetLand` clears
  historical water even when the modern observation was already land.
  Uncorrected modern land/unknown evidence does not erase historical water.
- New modeled packages use recipe 6; model-free overview uses recipe 5.
  Model indexes 1 and 2 remain readable; unknown versions are rejected.
- Detailed water height sampling now uses acquired Copernicus DEM pages at
  the bounded hydrology grid, rather than ETOPO overview elevations.
- Root fixed a real HydroLAKES ingestion bug: gdal-rs calls feature count while
  constructing an iterator, and OpenFileGDB consumes the filtered cursor.
  `sampling.rs::vector_features` now resets that cursor after iterator creation
  through a second safe layer handle. A native FileGDB regression exercises it.
  Preprocessing now says `hydrology-gdal-page-v3`.
- Flat-background negative-infinity depth had hidden units in WebGPU and
  discarded background textures in Canvas. Both paths are fixed and browser
  regressions passed. Creator panel z-index was also raised above the status
  HUD after actual preview captures showed overlap; that CSS change still
  needs its next source-creator visual confirmation.

## Immediate blockers, in priority order

### 1. Recipe-6 activation is rejected by simulation

`crates/simulation/src/start_search.rs::search_start_for_recipe` accepts only
3, 4, and 5, then returns `EnvironmentPageError::Invalid`. Detailed packages
now use 6. The qualifier already accepts 5/6, so its reported failure was
`page residency failed: environmental page request is invalid`.
Temporary stage diagnostics proved the failure is `ordinary_start_search`,
**before page sampling**, not an independent-history-axis error.
Diagnostics were removed before this checkpoint.

Fix supported recipe validation in the owning simulation module while
preserving start-search semantics and caps. Add a recipe-6 activation/start
regression and unknown-version rejection. Audit other recipe allowlists.
Then run the ordinary detailed creator/server activation path, not just the
qualification helper. Do not work around this by lying about the recipe.

### 2. Lake rasterization is unfinished and unwired

`crates/geodata/src/hydrology/sampler/lakes.rs` is a new **uncompiled, untested,
unwired** 191-line work-in-progress file. It contains a bounded GDAL MEM page
rasterizer and overlap/hole/multipolygon/boundary/limit tests. No module
declaration or sampler call was added before the stop. A passing workspace
gate does not validate this file until it is wired.

Why needed: after fixing the cursor, point-by-point GEOS containment repeatedly
rebuilds huge lake polygons. Finland at only 128 samples took several minutes;
1024 needs the efficient bounded raster path. Finish wiring the helper into
`sampler.rs`; preserve first-feature precedence, center-sample semantics,
typed reservoirs/regulated lakes, holes, 64×64 memory bounds, and river topology.
Keep `all_touched` disabled. Check geometry-copy memory against existing bounds.
Compare with non-boundary vector containment and regenerate Finland 128/1024.
Do not lower resolution or change the fixed location to avoid the problem.

### 3. Full visual qualification has not run

The harness now creates an extra source-backed Alpine case: same 30 km center
46.6°N/10.5°E, compression 20, 750 tiles, 24×24=576 chunks. It retains existing
64-page/1 MiB source-page limits. Browser helpers intend to traverse beyond the
512-chunk cache, return to southeast high relief, observe exact-URL refetch,
and verify recovered terrain on both backends. This has not executed yet.

The harness was repaired to decode compact and legacy historical coverage
through `aoe_map::HistoricalLandUsePage`. Its tests and browser lint/typecheck
passed. The next run generated and verified Alpine package
`fb6698de80fb31615a23bd3b9b7b651ab849e354188154dd37494a30043c3f72`,
then failed at server startup: `invalid AOE_ASSET_PACK root: No such file or directory`.

The local pack **does have** `manifest.json` (6,327,364 bytes). The agent's
initial missing-manifest diagnosis was incorrect. `FINAL/local-assets/packs`
is a symlink into the original checkout, so mount the entire projects parent
using the ROOT_MOUNTS command below; otherwise the symlink target is absent
inside Docker. Verify container-visible paths before regenerating everything.
Do not put private art in Git or container images. No browser eviction captures
were produced. Preserve the dated coast correction record; Nile is a coastal
case, not a demonstrated estuary. Do not restore the stale package-hash pin.

### 4. Final source navigation and scales remain unqualified

Fixed requests are in `reports/geodata/qualification-requests` (ignored).
They can be reconstructed as schema1/year600/seed1/ratio1:1, Paris center
48.85°N,2°E with sides 1024,32768,100000,524288 m; Sahara center25°N,-5°E,
side100000m. Use standard/circa600_v1 profiles.

All five overview packages generated. Paris 100km overview has only44 pages,
so correctly fails the unchanged129-page eviction requirement. The original
qualification primary was detailed1024, as was the16384-tile Paris reference.
Regenerate those two with `map-generate-detailed`, `AOE_MAP_SAMPLES=1024`,
after the lake fix. Sahara stays overview128. Other overview scale references
can be used if their explicit contracts are satisfied; label their preparation.

Use Paris primary plus Sahara geographic reference in `map-source-qualify`.
Preserve the unchanged Paris diagnostic (previously SearchLimit/tree boundary)
and predetermined Sahara five long orders, not hand-picked replacement routes.
Verify movement-driven page residency, replay, resources, work and memory caps.
The previous Sahara100km result belongs to an older recipe/source revision.

## Evidence achieved in this session, before the stop

These are intermediate working-tree results, not final clean-head qualification.

| Check | Result / limit |
| --- | --- |
| Combined geodata |179 tests passed (172 lib,1 binary,6 integration), including FileGDB cursor regression; predates unwired lake rasterizer |
| Map tests |114 passed |
| Browser type/lint |Passed after visual helper edits |
| WASM browser tests |Passed after background-depth fixes |
| Private-art ordinary E2E |36 passed,5 skipped; before final source fixes |
| Source creator |Create/verify/activate/reload/restart/offline unseen chunk/resource persistence passed for overview Paris; rerun latest head, plus detailed recipe6 |
| Fixed source matrix |11/11 PASS, including Fiji date line and maximum10000km; `reports/geodata/matrix.json` |
| Map performance |Synthetic fallback5 chunks/3328 tiles/371 resources,258ms; not RTS qualification |
| perf-ci |Instruction/size portions ran, then failed on in-progress harness compilation; no combined pass |
| Water branch preflight |`b31561b`:579/579, doctests, policy, perf-smoke and hooks passed; not combined head |
| Historical branch preflight |589 tests observed passing; final exit lost, not a claimed complete pass |
| Fuzz diagnostic campaign1 |PASS on clean `da654044fefa0ae39eeb73324710df8b3878c90a`,7 targets×300s; not final parser revision |

Diagnostic fuzz corpus:11,025 files/2,910,736 bytes, below unchanged16,384-file/
64MiB caps, no crash artifacts. Preserved report:
`AoeWorld-fuzz/reports/fuzz/nightly-campaign-1-da654044.json`, SHA256
`8c8a2a44a1c32fdca14eb12017da22384d507de7dae053e120f5c05036451573`.
Two complete campaigns on the eventual clean integrated parser revision are
still required. Add canonical recipe6/model-v2 and legacy-v1 water-model
package/page seeds first; current seed builders set water_model=None.
Retain model-free and schema compatibility seeds and compact history coverage.

Real detailed Paris1024, before the lake cursor fix:
`a5b50c2df0038a91a64f524c6c171ee091e9bdf2e7085e1ba9f498891d0d9ae3`:
19 source locks,13,199 river samples,11,510 directed edges all checked level or
downhill,1,689 explicitly unknown directions. This does not qualify lakes.
Corrected Finland128 before the v3 marker/optimization:
`cd17f342f7cb41be61e883ba17eb593940aca2bb8ac81109a8b34df423f9a1a5`:
4,751 lake samples; largest connected component3,613 samples at84.77m.
Earlier Finland1024 `223cf3aa...` had zero modeled lakes and is **bad evidence**.

## Local paths and commands

Keep geodata, art, generated packages, reports and fuzz artifacts outside Git.
They are not preserved merely by pushing PRs; copy them separately if moving
machines. Do not delete the old worktrees/caches until their ignored data is saved.

```sh
cd /home/anko/Work/projects/AoeWorld-final
export AOE_GEODATA_CACHE=/home/anko/Work/projects/AoeWorld/.cache/geodata
export AOE_ASSET_PACK=/home/anko/Work/projects/AoeWorld-final/local-assets/packs/7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce
make ROOT_MOUNTS='-v /home/anko/Work/projects:/home/anko/Work/projects' geodata-test
```

Use that same ROOT_MOUNTS override for Make, commits and pushes. For hooks:
`MAKEFLAGS='ROOT_MOUNTS=-v\ /home/anko/Work/projects:/home/anko/Work/projects' git commit ...`.
Set `AOE_MAP_REQUEST` to an absolute JSON path and `AOE_MAP_PACKAGE` to an
isolated output directory; use `map-generate` or `map-generate-detailed`.
`map-verify` accepts a manifest or package directory as supported by the CLI.

- Source cache: original checkout `.cache/geodata`, roughly7.7GiB.
- Original art: original checkout `local-assets/packs/<hash>`; symlink in FINAL.
- Matrix packages: FINAL `local-assets/maps-matrix`; fixed requests are tracked
  in `docs/geodata/reference-matrix.json`.
- Detailed diagnostics: FINAL `local-assets/detailed-water-*`.
- Navigation packages: FINAL `local-assets/source-qualification/*`.
- Earlier navigation inputs/reports: original `.cache/map-completion/source-qualification`.
- Logs: `/tmp/aoeworld-finish-*`; source qualification logs distinguish the
  overview129-page failure from the detailed recipe6 failure.
- Fuzz artifacts/corpus: `AoeWorld-fuzz/fuzz`, reports in its `reports/fuzz`.

For full navigation set `AOE_SOURCE_QUAL_PACKAGE_DIRECTORY` and
`AOE_SOURCE_QUAL_CONTENT_HASH` to the new detailed Paris primary; set matching
`AOE_SOURCE_QUAL_GEOGRAPHIC_*` to Sahara. Set `AOE_SOURCE_QUAL_512_*`,
`AOE_SOURCE_QUAL_16384_*`, `AOE_SOURCE_QUAL_262144_*` for scale references.
Run `map-source-qualify` and, when isolating scale contracts,
`map-source-scale-qualify`. Do not mix directories with stale hashes.

## Finish sequence and publication

1. Fix recipe6 start search with a real detailed activation regression.
2. Wire/test bounded lake rasterization; verify source Finland1024 and Paris
   river/lake/junction/correction behavior. Ensure stable source/model identities.
3. Run detailed creator end to end and repair container mounts for source
   visuals. Inspect both backend images, sprite contacts, banks, relief, and
   actual eviction/refetch. Fix actual anomalies; do not just accept screenshots.
4. Regenerate detailed qualification inputs; rerun unchanged geographic routes,
   resource lifecycle, page eviction, replay and all scale contracts.
5. Freeze code. Run hooks, preflight, map-test, geodata-test, browser-check,
   test-wasm, test-e2e, test-creator-source, test-geographic-matrix,
   test-geographic-visuals, coverage/coverage-check, fuzz-smoke, two full
   fuzz-nightly campaigns, perf-ci and map-perf. Coverage floors remain85%
   overall/90% critical. Coordinate heavy suites to avoid resource contention.
6. Update `docs/geodata/completion.md` with exact clean revision, commands,
   reports, hashes, limits and failures. The existing ledger is not current.
7. Obtain CI on a final PR against supported base `dev` (or required project
   integration base). PR72 is still a preservation draft against the feature
   base; old stacked PRs do not collectively prove combined acceptance.
   Only then mark the final PR ready. Do not claim dedicated hardware/RTS
   qualification from synthetic workloads or sparse maximum-size tests.

No source-generating, browser, fuzz, or agent jobs were intentionally left
running at the stop. Preservation hook checks are recorded in the PR update.
