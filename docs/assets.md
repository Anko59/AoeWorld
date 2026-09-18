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
`AOE_ASSET_PACK=local-assets/packs/<pack-hash>` to play at `/`. For inspection, open `http://127.0.0.1:8080/asset-viewer.html`. The game uses terrain resource 15008, cavalry walking/standing resources
3008/3004. It preserves frame anchors, player color, and shadows; gameplay
terrain is uniform grass and does not add decorative border trees. The viewer
renders imported frames and their masks.
Synthetic diagnostics are at `/diagnostics.html`. Stop an existing
server with `make down` before changing the selected pack. Public builds do not
contain trial files or local packs. Fixture tests run in public CI; actual trial
import and visual inspection are reported separately.
