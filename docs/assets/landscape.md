# Stage 1 landscape catalog foundation

Catalog v4 keeps approved art separate from discovery. Required sources are
cavalry 3008/3004 and five terrain sheets; optional broadleaf resources and ten
forest-floor accents are unchanged. The reviewed selection ceiling is now 656
frames (formerly 756). No multi-atlas support or candidate loading is added.
This count is a selection ceiling, not proof of arbitrary atlas packing success.

## Actual local evidence

Inspected metadata from private manifest version 1, converter 0.1.0, input hash
`7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce`.
Every candidate below exists and its source frame numbers are contiguous from
zero. Source identity includes archive plus DRS kind `[32, 112, 108, 115]` plus ID.
No original pixels, generated manifests, or screenshots are public fixtures.

| Candidate | Archive | Frames | Evidence and limits |
| --- | --- | ---: | --- |
| Conifers 4654 | graphics.drs | 9 | Existing private contact sheet inspected across all nine; includes green, bare, and autumn-colored variants. No runtime variant selection approved. |
| Possible shadows 2304 | graphics.drs | 9 | Frame 3 screenshot shows shadow-shaped art, 80×168, hotspot (66,153). Equal counts do not establish pairing with 4654. |
| Palms 4653 | graphics.drs | 13 | Dimensions 61–129 wide, 75–168 high; signed source hotspots preserved. Metadata only. |
| Grasses 15001/15009/15006 | terrain.drs | 100 each | All 97×49, hotspot (0,0). Names are proposals; no seam or color review here. |
| Beach 15017 | terrain.drs | 100 | 97×49, hotspot (0,0); shore compatibility unreviewed. |
| Shallows 15014 | terrain.drs | 100 | 97×49, hotspot (0,0); depth mapping unreviewed. |
| Water 15015/15016 | terrain.drs | 100 each | 97×49, hotspot (0,0); seams, animation, and depth roles unreviewed. |
| Ice 15024 | terrain.drs | 64 | 97×49, hotspot (0,0). Neither 100-frame indexing nor assumed 8×8 periodicity is justified. |
| Cliffs 226–234 | graphics.drs | 25 each | Mixed bounds include 1×1/2×3 placeholders and far-away hotspots. 227 includes a 329×718 frame. Must inspect valid variants and placement before use. |
| Cliff 235 | graphics.drs | 1 | 154×79, hotspot (96,32); metadata only. |

These 21 entries live in `catalog::candidates::UNAPPROVED_SOURCES`, not in any
reviewed render list. Their names describe the requested investigation, not
proof that those IDs have the proposed semantic role. Existing screenshots are
historical inspection evidence, not a newly captured running-game qualification.

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
