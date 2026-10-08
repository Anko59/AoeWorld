# Assets

Trial assets and derived packs stay in ignored `local-assets/`, outside Docker
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
validates every local pack. The Rust importer bounds DRS, palette, and SLP
decoding, creates deterministic padded PNG atlas pages with separate player,
shadow, and outline masks, and writes a versioned manifest.
Importer-generated pages use RGBA8 with transparent black, opaque color and
player pixels, and black shadows at alpha 128. The browser client reconstructs
PNG samples directly, preserving the full RGBA values allowed by manifest
version 1, including arbitrary partial alpha in externally produced packs.

To inspect a pack in the local app, start `make dev` with
`AOE_ASSET_PACK=local-assets/packs/<pack-hash>` to play at `/`. For inspection, open `http://127.0.0.1:8080/asset-viewer.html`. The game imports cavalry walking/standing resources
3008/3004 and five reviewed imported terrain groups: temperate grass 15008,
dry grass 15007, dirt 15000, sand 15010, and water 15002. These groups declare
10×10 periodic topology explicitly, indexed x-major with reversed y so
neighboring source edges meet. Catalog v5 retains the removal of paved 15018 from the natural
rock role: natural rock requires procedural neutral ground, not paving. The
renderer owns that fallback's appearance. Frame count alone is never topology. The map client selects those groups from
semantic terrain chunks. Optional native Forest/g_for terrain 15011 contributes
frames 0–9 as nonperiodic coordinate-stable forest-soil frames. V2 forest beds
blend this native leaf litter with grass, dry grass or authoritative dirt; only
absent optional forest art retains the prior dirt fallback. These reviewed frames
are not qualified as a seamless periodic sheet. Legacy material 6 selection is unchanged.
The five periodic groups retain all 100 frames. Catalog v5 selects at most 678 frames:
510 terrain and 168 objects. Runtime packing remains three 2048² RGBA pages/48 MiB,
terrain on pages 0/1 and objects on page 2, with the 2048-frame hard cap unchanged.
Counts alone do not prove fit; private full-manifest packing must be verified.
It preserves raw signed frame anchors, player color and imported shadow masks;
broadleaf 4652 uses only its paired shadow-only frames from 2296. Cavalry and non-tree resource
frames without imported shadows use a fixed-direction fallback silhouette
derived from their selected sprite alpha. The renderer culls terrain to the
viewport and subsamples it to a bounded sprite budget.

The [landscape inventory](assets/landscape.md) records reviewed subsets and
unpromoted candidates separately. Species promotion does not expand the existing
three-page runtime atlas allocation.

The current local pack is intentionally a narrow gameplay mapping. Its
versioned catalog renders four visually reviewed resource roles when their
optional sources are present: berry bushes for food, broadleaf trees for wood,
gold deposits, and stone deposits. Wood family 2 selects conifer 4654 healthy
frames [1,2,3,4,7,8]; family 4 selects palm 4653 singles [0,1,2,3,5,6,8,10,11,12].
The loader retains full raw prefixes 9/13, but bare/autumn conifers and palm clumps
are not resource presentations. These two species use the selected body alpha
silhouette, never broadleaf 2296 or unqualified 2304/2300 shadows. Missing optional
species keeps the healthy broadleaf fallback; family 0 keeps exact legacy variants.
Culling and drawing share the selected body and its shadow decision. Palm frame 0
anchor (20,175) lies below its 88×168 image and is deliberately preserved, not clamped.
Animal units, buildings, and other
terrain-object art remain unavailable until independently reviewed. A missing
required terrain group prevents local-pack startup with a clear error, while a
missing optional resource group does not turn unrelated frames into a
substitute. Optional absence is distinct from present incomplete/duplicate frames,
invalid bounds or corrupt PNG: those remain startup errors, not visual fallback. Synthetic diagnostics retain their grass fallback only before map
chunks arrive. The viewer renders imported frames and their masks.
Synthetic diagnostics are at `/diagnostics.html`. Stop an existing
server with `make down` before changing the selected pack. Public builds do not
contain trial files or local packs. Fixture tests run in public CI; actual trial
import and visual inspection are reported separately.
