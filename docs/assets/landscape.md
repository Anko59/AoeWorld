# Reviewed landscape catalog and candidates

Catalog v11 keeps approved art separate from discovery. Required sources remain
cavalry 3008/3004, five base terrain sheets and soft snow 15026. Optional full conifer 4654 and palm 4653
prefixes, three water depth sheets, four ground-variety sheets, rock 15033
and ice 15024 bring the ceiling to 1,674 frames
(1,474 terrain/200 objects including cliff faces).
The existing runtime cap remains 2048 frames and three 2048² RGBA pages/48 MiB;
terrain occupies pages 0/1 and objects page 2. Counts are not packing proof.
Private original-manifest extents must fit with no overlap, truncation or cap change.
Family 0 and absent-species fallback retain their base semantic selection;
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
| Grasses 15001/15009/15006 | terrain.drs | 100 each | Promoted in v7. All 97×49, hotspot (0,0). Montages in x-major, reversed-y order tile without seams: 15001 darker grass, 15009 grass with brown patches, 15006 grass and dirt mix. |
| Beach 15017 | terrain.drs | 100 | Promoted in v7 as gravel: 97×49, hotspot (0,0), seamless pebbly montage. Shore compatibility remains unreviewed. |
| Ice detail 15024 | terrain.drs | 64 | 97×49, hotspot (0,0), seamless 8×8 x-major, reversed-y sheet. Blue-white cracked ice for frozen water only; never snow ground. |
| Snow 15026 | terrain.drs | 100 | Conquerors trial; 97×49, hotspot (0,0), soft white ground, seamless 10×10 x-major, reversed-y sheet. |
| Rock 15033 | terrain.drs | 100 | HD native rock, obtained privately as WololoKingdoms `DLC_ROCK.slp`; 97×49, hotspot (0,0), broad warm/cool stone relief, seamless 10×10 x-major, reversed-y sheet. Absent from the trials. |
| Cliff faces 226 | graphics.drs | 25 | Reviewed over a three-by-three tile grid and promoted: slots 1,2,3,4,5,15,16,17,18,19,24 are rock faces falling toward the camera (270–308 × 136–156, hotspot at the footprint centre); the other fourteen slots are 1×1/2×3 placeholders. |
| Cliffs 227–234 | graphics.drs | 25 each | 227 (slots 8,22,23) and 228 (10,11,20,21) are thin rims of cliffs seen from behind; 229–232 hold one corner piece each; 233/234 one small crag each. Mostly placeholders; 227 slot 7 is a 329×718 frame. Unpromoted. |
| Cliff 235 | graphics.drs | 1 | 154×79, hotspot (96,32); metadata only. |

The remaining 10 discovery entries live in `catalog::candidates::UNAPPROVED_SOURCES`;
conifer/palm prefixes and the four ground sheets have moved to the reviewed optional load list. Their
resource variants are constrained to the subsets above. Names or equal frame
counts never establish roles or shadow pairing. Viewer screenshots are inspection
evidence, not backend/packing qualification. An explicit native private-manifest
test separately feeds all 1,674 actual extents through the runtime packer; the
ordinary portable suite's ignored private test is not a successful proof. Explicit
invocation requires `AOE_REVIEWED_PACK_MANIFEST` and fails if input is absent.
An explicit browser proof requires `AOE_TREE_ART_PROOF=1` and an output directory:
it uses original pack art with synthetic flat terrain/resources, asserts all three
backends and visible pixel removal, not generated ecosystem/source fidelity. Loader
absence uses the existing healthy broadleaf fallback; present incomplete,
duplicate or corrupt sources fail rather than masquerading as absence.

## Water depth sheets

Inspected in the local pack as 7×7-tile periodic patches; all four are 100
frames of 97×49 in the same x-major, reversed-y layout as the land sheets.

| Sheet | Look | Scene material | Used for |
| --- | --- | --- | --- |
| Water 15002 (required) | Light turquoise | 10, 12 | Shallow rim of lakes and seas, river margins, shallow source water, and playable Ford crossings |
| Water 15016 (optional) | Medium blue | 5 | Water five to seven tiles from land at most; midstream river water |
| Water 15015 (optional) | Dark blue | 11 | Open water beyond the medium reach |
| Shallows 15014 (optional) | Green marsh with lilies | — | Imported, not selected for Ford crossings |

Generated river fords use the light shallow Water 15002 sheet where local
slopes permit crossing; marsh flecks no longer obscure the crossing.
Ordinary shallow water and river margins stay impassable.

Faces between two classes blend their sheets through the same per-vertex
splat as land materials; water never blends with land across the shore. No
water or shore face uses a procedural flat tint. Gravel 15017 is approved only for land:
the shore band uses reviewed sand 15010. Animation is not implemented.
## Cliff pieces

Cliff records mark three-by-three impassable cells along steep contours,
with the same open passes. All nine tiles share the uppermost crossed contour
level. Neighboring cells along that contour cannot create internal box steps.
The current surface tag distinguishes these ledges from continuous steep
ground, which keeps its shared sloping corners at every mesh LOD.
The snowy or grassy top meets a closed mesh face at the same world vertices;
per-vertex depth replaces the constant-depth cliff billboard.

East and south walls sample ten reviewed opaque rock interiors from the original
cliff source. A world-coordinate hash chooses compatible lighting groups, and
one formation spans three tiles rather than repeating a stripe on every tile.
The selected rectangles retain native strata, highlights and dark crevices while
excluding transparent silhouette holes. Source lighting and the runtime atlas
palette correction remain intact. Without complete face art, walls retain their
procedural rock fallback.

Shared presentation vertices bevel outer and inner corners, roughen the rim,
and carry the same irregular outline to the foot. Both ground sheets and the upright
faces use the same lateral deformation, after integer material ownership is assigned.
Height-dependent offsets are avoided so crossed sheets keep their intersections.
An edge whose height difference changes sign closes with two triangles meeting
at the true crossing; uphill ground seams keep their ground appearance.
Coarse art-patch vertices interpolate the coarse edge deformation, keeping the
wall closed at minimum zoom inside the existing triangle bound. Missing halos
retain exact planes until the support is loaded. Isolated contour cells are
omitted at placement; joined pairs and ridge ends remain deterministic.
Picking uses presented depth but returns the corresponding source ground
coordinates. Source span and triangulation occupy unused bits of the private
order byte; ordinary draw order and the triangle/GPU packet sizes stay unchanged.

Native terrain diamonds have staircase alpha masks. At the continuous mesh
edge only, a transparent nearest texel samples one texel inward toward the
frame centre. This closes snow pinholes without changing object silhouettes,
fully transparent textures, or coverage outside the diamond.

## Loose rocks

`Stone` decorations draw graphics 1034: seven plain grey rock piles (56–85 ×
33–46), the stone-deposit shapes without ore glints. The decoration variant
selects the larger frames for three quarters of piles, retaining all seven
shapes and their native hotspots. Emission and culling use the shared object
scale of 128/96, with the soft shadow of objects without paired shadow art.
Crags 233/234, the rock slab 235 and the
boulders painted into the cliff faces are not used as separate objects.

## Snow ground

Snow (scene material 7) draws soft ground 15026 as authored, including mixed
borders. Frozen water (material 8) draws the separate blue ice 15024 sheet.
The native-ice flag avoids per-pixel directory checks. Snow no longer uses an
ice-detail or whitening kernel. Climatic snow and bare rock stay authoritative
over drawn variety, with no forage on either.

Forage bushes are filtered against composed ground after snow-line and shore
selection: none stands on rock, snow or ice. Mineral deposits are retained.

## Natural rock correction

Paved terrain 15018 and pebbly gravel 15017 are not mountain ground. Rock
(scene material 4) draws native 15033 with its authored stone colours and broad
structure. A missing optional rock sheet uses dirt detail, without a grey
kernel. Vertical closures and absent cliff-face art retain their stone tint.
Rock and snow use the same incident-tile coverage, ordered noisy thresholds
and world-tile sampling as grass and dirt on all three backends. Coarse patches
that meet mountain borders subdivide within the fixed triangle budget to keep
the source-tile blend band; pure patches keep ordinary LOD geometry. Pure and
mixed patches use the same displayed-vertex material support.

## Explicit topology contract

`SpriteSource::terrain_topology` is optional for nonterrain sprites. The reviewed
five periodic sheets use `PeriodicXMajorReversedY { columns: 10, rows: 10 }`.
`supports_frames` rejects zero dimensions and incomplete selected grids.
`periodic_frame` validates that range, wraps signed world coordinates, and uses
`x * rows + (-y mod rows)`; row zero retains the existing authored phase at
world y=0. It returns no index for independent accents.
Forest-floor frames 0–9 use `CoordinateStableAccents`. A 100-frame source may
be accents. The 64-frame ice sheet 15024 is an 8×8 grid by review, not by its
count. Numeric counts never confer seam review.

Before promotion, inspect full candidate masks, hotspots, usable variant subsets,
paired shadow alignment, sheet seams/order and actual WebGPU/Canvas gameplay.
Any promotion changes the catalog version and selection/packing budget tests.
Portable parser/topology fixtures and private pack verification are distinct
from these outstanding visual qualifications.
