# Reviewed landscape catalog and candidates

Catalog v5 keeps approved art separate from discovery. Required sources remain
cavalry 3008/3004 and five terrain sheets. Optional full conifer 4654 and palm 4653
prefixes add 22 frames to the prior 656 ceiling: 678 total (510 terrain/168 objects).
The existing runtime cap remains 2048 frames and three 2048² RGBA pages/48 MiB;
terrain occupies pages 0/1 and objects page 2. Counts are not packing proof.
Private original-manifest extents must fit with no overlap, truncation or cap change.
Legacy family 0 and absent-species fallback retain their original semantic selection;
a present species can repack atlas addresses, not change resource authority.

## Actual local evidence

Inspected metadata from private manifest version 1, converter 0.1.0, input hash
`7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce`.
Every candidate below exists and its source frame numbers are contiguous from
zero. Source identity includes archive plus DRS kind `[32, 112, 108, 115]` plus ID.
No original pixels, generated manifests, or screenshots are public fixtures.

| Candidate | Archive | Frames | Evidence and limits |
| --- | --- | ---: | --- |
| Conifers 4654 | graphics.drs | 9 | All nine originals inspected. Wood family 2 uses only healthy [1,2,3,4,7,8]; bare 0/6 and autumn 5 excluded. Selected-body alpha silhouette, not unqualified 2304. |
| Possible shadows 2304 | graphics.drs | 9 | Frame 3 screenshot shows shadow-shaped art, 80×168, hotspot (66,153). Equal counts do not establish pairing with 4654. |
| Palms 4653 | graphics.drs | 13 | All originals inspected. Wood family 4 uses singles [0,1,2,3,5,6,8,10,11,12], not clumps 4/7/9. Signed/outside hotspots preserved, including frame 0 (20,175) on 88×168. Selected-body alpha silhouette, not 2300 or broadleaf 2296. |
| Grasses 15001/15009/15006 | terrain.drs | 100 each | All 97×49, hotspot (0,0). Names are proposals; no seam or color review here. |
| Beach 15017 | terrain.drs | 100 | 97×49, hotspot (0,0); shore compatibility unreviewed. |
| Shallows 15014 | terrain.drs | 100 | 97×49, hotspot (0,0); depth mapping unreviewed. |
| Water 15015/15016 | terrain.drs | 100 each | 97×49, hotspot (0,0); seams, animation, and depth roles unreviewed. |
| Ice 15024 | terrain.drs | 64 | 97×49, hotspot (0,0). Neither 100-frame indexing nor assumed 8×8 periodicity is justified. |
| Cliffs 226–234 | graphics.drs | 25 each | Mixed bounds include 1×1/2×3 placeholders and far-away hotspots. 227 includes a 329×718 frame. Must inspect valid variants and placement before use. |
| Cliff 235 | graphics.drs | 1 | 154×79, hotspot (96,32); metadata only. |

The remaining 19 discovery entries live in `catalog::candidates::UNAPPROVED_SOURCES`;
conifer/palm prefixes alone have moved to the reviewed optional load list. Their
resource variants are constrained to the subsets above. Names or equal frame
counts never establish roles or shadow pairing. Viewer screenshots are inspection
evidence, not backend/packing qualification. An explicit native private-manifest
test separately feeds all 678 actual extents through the runtime packer; the
ordinary portable suite's ignored private test is not a successful proof. Explicit
invocation requires `AOE_REVIEWED_PACK_MANIFEST` and fails if input is absent.
An explicit browser proof requires `AOE_TREE_ART_PROOF=1` and an output directory:
it uses original pack art with synthetic flat terrain/resources, asserts all three
backends and visible pixel removal, not generated ecosystem/source fidelity. Loader
absence uses the existing healthy broadleaf fallback; present incomplete,
duplicate or corrupt sources fail rather than masquerading as absence.

## Natural rock correction

Paved terrain 15018 is deliberately removed from the reviewed natural-rock
role, following the user's identified semantic mismatch. Its local metadata
still contains 100 contiguous 97×49 frames, hotspot (0,0); that does not make it
natural rock. No substitute rock sheet is approved. The catalog explicitly
requires procedural neutral natural-rock ground, never another unrelated sheet.
Renderer integration and appearance are owned separately; this stage does not
claim a new paving pixel review or a verified running-game fallback.

## Explicit topology contract

`SpriteSource::terrain_topology` is optional for nonterrain sprites. The reviewed
five periodic sheets use `PeriodicXMajorReversedY { columns: 10, rows: 10 }`.
`supports_frames` rejects zero dimensions and incomplete selected grids.
`periodic_frame` validates that range, wraps signed world coordinates, and uses
`x * rows + (-y mod rows)`; row zero retains the existing authored phase at
world y=0. It returns no index for independent accents.
Forest-floor frames 0–9 use `CoordinateStableAccents`. A 100-frame source may
be accents; a 64-frame grid is supported by the type but ice is not approved
as such. Numeric counts never confer seam review.

Before promotion, inspect full candidate masks, hotspots, usable variant subsets,
paired shadow alignment, sheet seams/order and actual WebGPU/Canvas gameplay.
Any promotion changes the catalog version and selection/packing budget tests.
Portable parser/topology fixtures and private pack verification are distinct
from these outstanding visual qualifications.
