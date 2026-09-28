---
name: game-assets
description: Import, inspect, integrate, or troubleshoot Age of Empires II sprite and terrain art in AoeWorld, including atlas masks, cavalry animation, and local visual verification. Use for game-art work; use the rendering guide for unrelated WebGPU work.
---

# AoeWorld game assets

Use the repository's Rust importer and local atlas pack. The downloaded trial,
extracted files, imported PNGs, and screenshots containing original game art
belong in ignored `local-assets/` or another private local directory, never in
Git, Docker images, or public artifacts. Source licensing is separate from the
MIT-licensed code. Synthetic fixtures are the portable CI evidence; say
separately when actual trial data was tested.

Read [docs/assets.md](../../docs/assets.md),
[docs/agent-engineering.md](../../docs/agent-engineering.md), and the scoped
`AGENTS.md` for every crate you edit. For archive decoding, pack creation,
manifest fields, commands, and private-data boundaries, read
[the pack pipeline](references/pack-pipeline.md). For adding an animated unit or
debugging its appearance, read [the rider rendering example](references/rider-rendering.md).

## Choose the path

- **Discover/import:** Run `make assets-inspect`, then `make assets-import` if
  the trial is present. The content-addressed result lives under
  `local-assets/packs/<input-hash>/`. Run `make assets-verify` before loading it.
- **Inspect visually:** Start `AOE_ASSET_PACK=local-assets/packs/<input-hash> make dev`.
  Use `/asset-viewer.html` to filter resource IDs, step frames, change player
  color, and inspect dimensions and anchors. Use `/` to check the actual game
  animation, ground alignment, facings, and both rendering backends. Stop the
  server with `make down` before selecting a different pack.
- **Integrate:** Select frames by archive/resource ID and ordered frame number,
  keep each frame's atlas rectangle and hotspot, and apply player/shadow masks
  deliberately. Reuse atlas textures across units and frames; move entities
  through world coordinates rather than expanding the atlas or world-sized
  sprite arrays.

Never assume a resource's frame count or direction order from its numeric ID.
Inspect the pack and verify contiguous frames before indexing. The 3008/3004
layout in the worked example is observed in the tested trial pack; it is not a
general promise for all AoE editions or resources. Reject unsupported formats
and malformed input explicitly instead of guessing or silently skipping it.

For importer changes, exercise parser fixtures and `make assets-verify` on a
local pack when available. For client or renderer changes, run `make build-wasm`
and `make test-e2e` to check visible pixels, movement, and WebGPU/Canvas 2D
startup; follow the scoped rendering checks. Before committing, run
`make hooks-install`, `make hooks-check`, a focused gate, and `make preflight`.
Report the exact revision and what was validated with fixtures versus private
trial data. Do not use a synthetic workload as proof of finished large-scale
RTS performance.
