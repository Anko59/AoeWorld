# Local pack pipeline

Read this when discovering, importing, verifying, or changing the game-art
pipeline. Canonical behavior lives in `crates/assets/src/{drs,slp,palette,pack}.rs`,
`crates/harness/src/main.rs`, `crates/server/src/config.rs`, and
`docs/assets.md`; revisit those files if the implementation changes.

## Source to pack

The tested input is the Age of Kings trial extracted under
`local-assets/trial/`; the original installer and archives remain private.
`docs/assets.md` records the tested installer hash and extraction procedure.
The default import chooses `local-assets/trial/Data` if present, otherwise
`local-assets/trial`. This matters: the game DRS archives and canonical JASC
palette 50500 must be converted together. The separate campaign media has
multiple palettes and is outside the default pack.

From the repository root:

```sh
make assets-inspect
make assets-import
make assets-verify
```

These are Dockerized Rust harness commands. `assets-inspect` inventories the
entire trial directory, so unsupported non-art files can appear in its report.
To specify a different extracted input or verify one pack, use the equivalent
harness CLI through the pinned tools image rather than adding logic to Make:

```sh
docker run --rm --init --user "$(id -u):$(id -g)" \
  -e CARGO_HOME="$PWD/.cache/cargo" -v "$PWD:$PWD" -w "$PWD" \
  aoeworld/rust-tools:1.93.1 \
  cargo run --locked -p aoe-harness -- assets import local-assets/trial/Data
```

The importer writes `local-assets/packs/<input-hash>/`, where the hash covers
sorted input names and bytes. Reimporting the same input verifies and reuses
that directory. Do not edit a generated page or manifest in place: a pack is
content addressed. The current directory key depends on input bytes, not the
converter version. After changing converter behavior, avoid mistaking an old
pack for new output: explicitly regenerate the ignored derived pack or revise
the key/version scheme as part of that change, then verify it. Preserve the
original trial input.

## What the manifest means

`manifest.json` version 1 has `converter`, `input_hash`, `pages`, and `frames`.
Each page names four 2048×2048 RGBA PNGs and their BLAKE3 hashes:

| Layer | Meaning |
| --- | --- |
| `color` | Palette-resolved ordinary sprite pixels. |
| `player` | Player-color mask; red is a palette shade index, alpha marks coverage. |
| `shadow` | Shadow coverage/opacity, separate from ordinary colors. |
| `outline` | Outline class in red and coverage in alpha. |

Each frame records `source`, `source_hash`, zero-based `frame`, `page`,
`x`, `y`, `width`, `height`, `anchor_x`, and `anchor_y`. The rectangle is the
crop within every layer of that page. The anchor is the original SLP hotspot
relative to the frame's top-left, not its geometric center. For ordinary
placement at screen point `(sx, sy)`, use top-left
`(sx - anchor_x, sy - anchor_y)` after applying zoom. For a horizontal mirror,
use `sx - (width - anchor_x)`; reverse the U texture coordinate too. Keep the
original signed hotspot values rather than clamping them.

The source identity has the form
`graphics.drs:[32, 112, 108, 115]:3008` in the tested pack. The byte array
is the DRS entry kind as rendered by Rust, and `3008` is its resource ID.
Filter by *both* archive and ID: an ID alone need not be globally unique.
Within one source, sort records by `frame`, confirm `0..count-1` with no gaps,
and only then form animation rows. The client enforces this for its three
current resources in `crates/client/src/game_assets.rs`.

The Rust importer bounds input sizes, DRS offsets, SLP frame count/dimensions,
atlas pages, and total frame count. Its verifier checks manifest schema,
unique source/frame pairs, page-local paths, rectangle bounds, PNG format,
and page hashes. Treat downloaded or user-supplied art as untrusted. A visual
viewer or JSON parser is not a replacement for `make assets-verify`.

## Local inspection and diagnosis

```sh
AOE_ASSET_PACK=local-assets/packs/<input-hash> make dev
```

Open `http://127.0.0.1:8080/asset-viewer.html`; filter `3008` for walking
cavalry, `3004` for standing, and `15008` for grass. The viewer shows one
frame at a time, lets you step frames, and composites shadow, color, player
color, and outline. Inspect frame 0 and frames across direction-row boundaries
(10, 20, 30, 40), then verify motion and facings in the actual game at `/`.
The viewer is a diagnostic; the client currently compacts selected source
pages into one runtime atlas and composites player/color/shadow differently.
Do not conclude gameplay looks correct solely from the viewer.

If the game reports missing resources, check the selected pack path and exact
source IDs before changing rendering code. If it reports a full runtime atlas,
review the selected-frame budget and packing strategy; increasing map size
must not increase the number of atlas pages loaded for one unit. If art looks
offset, inspect hotspots and mirrored placement before changing world or tile
coordinates. If player colors appear dull, inspect the mask's red values and
the runtime shade mapping; they are not full 0–255 RGB colors.

Use synthetic parser fixtures for reproducible CI and local trial data for
real-format/visual evidence. Never commit a trial-derived page, screenshot,
manifest, or contact sheet, even when it looks like a harmless test fixture.
