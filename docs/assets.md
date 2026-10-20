# Assets

Original assets and derived packs stay in ignored `local-assets/`, outside Docker
build contexts and public artifacts. Synthetic fixtures remain the default.
The original game data license is separate from AoeWorld's MIT code license.

For the Age of Empires II: The Age of Kings trial, download `AoE2demo.exe` from
the [GameFront trial page](https://www.gamefront.com/games/age-of-empires-ii-the-age-of-kings/file/age-of-empires-ii-the-age-of-kings-trial-version).
The tested installer is 49,083,656 bytes with SHA-256
`0f5df430b54a377a6cfd1e253169de2cf8a60a1751d6e8b72718d297bd45174c`.
An [Internet Archive copy](https://archive.org/download/AgeofEmpiresIITheAgeofKings_1020/AoE2demo.zip)
contains an installer with that same hash. Extract the installer without running
it, for example with `7z x -olocal-assets/trial local-assets/downloads/AoE2demo.exe`.
Installer extraction is a local preparation step, not part of the Rust importer.

`make assets-inspect` inventories the entire `local-assets/trial` directory.
`make assets-import` selects `local-assets/trial/Data` when it exists, so the
game DRS archives and canonical palette are converted together. For other
extracted layouts, it imports `local-assets/trial`; the Rust CLI also accepts an
explicit input directory. The separate campaign media has multiple palettes,
so it is not included in the default game sprite pack. `make assets-verify`
validates every local pack; `AOE_ASSET_PACK=<path>` selects one explicit pack.
`make assets-dat-table` decodes `empires2.dat` into a
local unit and building to sprite table ([game data](assets/game-data.md)). The Rust importer bounds DRS, palette, and SLP
decoding, creates deterministic padded PNG atlas pages with separate player,
shadow, and outline masks, and writes a versioned manifest.
Trial-imported pages use RGBA8 with transparent black, opaque color and
player pixels, and black shadows at alpha 128. DE player pixels retain their
mask intensity as alpha, blending the team shade over the colour plane.
The browser client reconstructs
PNG samples directly, preserving the full RGBA values allowed by manifest
version 1, including arbitrary partial alpha in externally produced packs.

To inspect a pack in the local app, start `make dev` with
`AOE_ASSET_PACK=local-assets/packs/<pack-hash>` to play at `/`. For inspection, open `http://127.0.0.1:8080/asset-viewer.html`. The game imports cavalry walking/standing resources
3008/3004 and five reviewed imported terrain groups: temperate grass 15008,
dry grass 15007, dirt 15000, sand 15010, and water 15002. These groups declare
10×10 periodic topology explicitly, indexed x-major with reversed y so
neighboring source edges meet. Water 15002 is the light shallow sheet; optional
periodic sheets add medium water 15016, deep water 15015 and marsh shallows
15014. Generated river `Ford` crossings draw with the light shallow 15002
sheet at 85% coverage over the bank dirt (docs/assets/landscape.md); the marsh sheet stays imported but is not selected for fords. Ordinary
shallow water remains impassable. A pack without an optional sheet draws the
shallow sheet in its place. Catalog v12 selects natural rock ground 15033 and soft snow ground 15026,
with distinct authored sheets and colours. Paved 15018 and gravel 15017 are
not rock substitutes; cracked ice 15024 is used only for frozen water. Frame count alone
is never topology. The map client selects those groups from
semantic terrain chunks. Optional native Forest/g_for terrain 15011 contributes
frames 0–9 as nonperiodic coordinate-stable forest-soil frames. Landscape forest beds
blend this native leaf litter with grass, dry grass or authoritative dirt; only
absent optional forest art retains the prior dirt fallback. These reviewed frames
are not qualified as a seamless periodic sheet. Metadata-free material 6 selection is unchanged.
Four optional ground-variety sheets, each a reviewed 10×10 periodic montage, add
grass and dirt mix 15006, grass with brown patches 15009, darker grass 15001 and
pebbly beach 15017; a pack without one draws dry grass, grass, grass or sand.
The periodic groups retain all 100 frames. Catalog v15's base selects 2,369 frames:
1,474 terrain and 895 objects including village art and cliff faces 226. The
[unit and wildlife selection](assets/units.md) adds 2,540 frames. DAT building
recipes add their selected native frames, including animated Mill layers,
static House variants and construction/wall angles. Runtime packing uses four
2048² RGBA pages/64 MiB, terrain on pages 0/1 and objects on pages 2/3,
with a 5,120-entry frame metadata cap.
Counts alone do not prove fit; private full-manifest packing must be verified.
It preserves raw signed frame anchors, player color and imported shadow masks;
broadleaf 4652 uses only its paired shadow-only frames from 2296. Cavalry and non-tree resource
frames without imported shadows use a fixed-direction fallback silhouette
derived from their selected sprite alpha. The renderer culls terrain to the
viewport and subsamples it to a bounded sprite budget.

The [landscape inventory](assets/landscape.md) records reviewed subsets and
unpromoted candidates separately. Species promotion does not expand the existing
runtime atlas allocation.

The Age of Kings trial lacks the new mountain sheets. Snow 15026 is present
in the Conquerors trial; the HD rock sheet is available as `DLC_ROCK.slp` in
WololoKingdoms conversion resources, imported privately as terrain 15033.
Both mountain sheets are optional: packs without them start with the base dirt
detail fallback; packs containing them draw their native ground art.

The current local pack is intentionally a narrow gameplay mapping. Its
versioned catalog renders four visually reviewed resource roles when their
optional sources are present: berry bushes for food, broadleaf trees for wood,
gold deposits, and stone deposits. Wood family 2 selects conifer 4654 healthy
frames [1,2,3,4,7,8]; family 4 selects palm 4653 singles [0,1,2,3,5,6,8,10,11,12].
The loader retains full raw prefixes 9/13, but bare/autumn conifers and palm clumps
are not resource presentations. These two species use the selected body alpha
silhouette, never broadleaf 2296 or unqualified 2304/2300 shadows. Missing optional
species keeps the healthy broadleaf fallback; family 0 keeps its base variants.
Culling and drawing share the selected body and its shadow decision. Palm frame 0
anchor (20,175) lies below its 88×168 image and is deliberately preserved, not clamped.
Optional village groups add the male villager: idle 1479, walking 1484,
wood chopping/carrying 1535/1536, berry foraging/food carrying 1490/1493,
gold mining 1545, stone mining 1560 and shared ore carrying 1552, each five
rows of fifteen frames.
The same eight-direction mirror mapping as the scout uses the native anchors.
Authoritative Gathering tasks select the resource tool even with a partial
load; Returning tasks select their carried resource, while idle/moving tasks
retain ordinary standing/walking art. Buildings use imported DAT recipes:
stand graphics and ordered deltas, annex tile displacements, shadows,
construction graphics, footprints and foundation terrain. The Town Center's
head is unit 109; unit 618 is its main-building annex, not the whole recipe.
Signed and outside anchors are preserved; player masks use the current blue
ramp. The foundation material is painted into the shared terrain before
tessellation, so ordinary terrain blending owns its edges in every backend.
See [game data](assets/game-data.md) for measured layouts and local proof tools.
These are metadata, not committed pixels. Missing village groups draw simple semantic markers;
a present incomplete group remains a pack error. The native metadata packing
proof covers the full base and actor prefixes plus DAT building and foundation
frames in the same four-page atlas.
Yellow wildflower 2561 and tan reed 4584 dressing are omitted because their
silhouettes read as gold and deadwood. Footprint wear modulates existing terrain
colors without additional faces. [Unit and wildlife art](assets/units.md) records
combat, hunting and death animations. [Dark Age building art](assets/buildings.md)
describes the optional DE overlay and construction stages. A missing
required terrain group prevents local-pack startup with a clear error, while a
missing optional resource group does not turn unrelated frames into a
substitute. Optional absence is distinct from present incomplete/duplicate frames,
invalid bounds or corrupt PNG: those remain startup errors, not visual fallback. Synthetic diagnostics retain their grass fallback only before map
chunks arrive. The viewer renders imported frames and their masks.
Synthetic diagnostics are at `/diagnostics.html`. Stop an existing
server with `make down` before changing the selected pack. Public builds do not
contain trial files or local packs. Fixture tests run in public CI; actual trial
import and visual inspection are reported separately.

## Definitive Edition buildings

The trial's `graphics.drs` lacks the Farm (SLP 417-419), the Lumber Camp
(3500-3503) and the Mining Camp (3488-3491). `make assets-import-de` fills them,
and the other Dark Age buildings, from an owner-licensed Age of Empires II:
Definitive Edition install (`DE_DIR`, default
`~/.steam/steam/steamapps/common/AoE2DE`, mounted read-only into the container).
It decodes `resources/_common/drs/graphics/<stem>_x1.sld` with the bounded SLD
decoder (`crates/assets/src/sld.rs`: BC1 colour, BC4 shadow and player-mask
layers, skip-previous-frame reuse) and writes a separate pack below
`local-assets/packs/`, previews to `local-assets/de-preview/` and the building to
pack-source mapping to `local-assets/de-table.md`. Nothing derived from the
install is committed. Sources are listed in `crates/assets/src/de/select.rs`
(West European camps, since no Dark Age camp exists). Frames are cropped to the
drawn layers and anchors move with the crop; the player plane stores a 0-7 ramp
from the colour layer's luma for every nonzero mask, with the mask value as alpha
to weight the blend. The trial's player plane uses the same ramp with alpha 255;
the client and DE previews share the blend calculation.
DE draws farms with terrain textures (`g_fm1`, `g_fm2`), imported at x1 scale as
opaque 1024x1024 frames.

Run `make test-assets-de` (or `make test-assets-de DE_DIR=/path/to/AoE2DE`)
for the real-house decoder and player-plane checks. This dedicated target takes
a heavy slot, mounts `DE_DIR` read-only at `/de-install`, and sets `AOE_DE_DIR`
inside the test container. It fails if the install is missing or no tests match.
Setting `AOE_DE_DIR` on the host for `make test-unit` does not forward it or mount
the install; ordinary unit tests use synthetic fixtures and skip real-file checks.
