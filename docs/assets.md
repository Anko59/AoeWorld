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

To inspect a pack in the local app, start `make dev` with
`AOE_ASSET_PACK=local-assets/packs/<pack-hash>` to play at `/`. For inspection, open `http://127.0.0.1:8080/asset-viewer.html`. The game imports cavalry walking/standing resources
3008/3004 and six terrain groups: temperate grass 15008, dry grass 15007,
dirt 15000, sand 15010, rock 15018, and water 15002. The map client selects
those groups from semantic terrain chunks. It preserves frame anchors, player
color, and shadows; the renderer culls terrain to the viewport and subsamples
it to a bounded sprite budget.

The current local pack is intentionally a narrow gameplay mapping. Its
versioned catalog renders the visually reviewed broadleaf-tree family as wood
resources when that optional source is present; food bushes, gold deposits,
stone deposits, animals, buildings, and other terrain-object art remain
unavailable until independently reviewed. A missing required terrain group
prevents local-pack startup with a clear error, while a missing optional
resource group does not turn unrelated frames into a substitute. Synthetic
diagnostics retain their grass fallback only before map chunks arrive. The
viewer renders imported frames and their masks.
Synthetic diagnostics are at `/diagnostics.html`. Stop an existing
server with `make down` before changing the selected pack. Public builds do not
contain trial files or local packs. Fixture tests run in public CI; actual trial
import and visual inspection are reported separately.
