# Cockpit slice 1: local evidence

The scope is the kingdom panel, saved views/landmark jumps and distributed
unit production. Far building placement and the first-zoom-out introduction
remain later work. Nothing was pushed or published.

## Asset and recording inputs

Verification uses only the repository's headless browser harness and the
repository showcase media image. No owner browser or desktop was used.
The original trial and owner-licensed DE sources were regenerated because the
available old pack lacked the current `replay_delay` field. Packs and all
screenshots/recordings remain ignored, outside Git and Docker images.

Current local pack identities:

- Base: `54916fcb9c1e43c7a63aa486e3c47824b24ba94c887168ed210517273626bff6`.
- DE overlay: `d52a695d618771323b9428f095aaca335fbdd53e802fd6cf6dcbec73077aece9`.
- Actor component: `31b7125c8bd6e407195bfdf4e7f2e508ead421b9e4f5428e68fdfd0ff15a7020`.

`make assets-import` and `make assets-import-de` completed. `make assets-verify`
passed for the base (20,396 frames, 27 pages) and overlay (18,179 frames,
two startup pages plus indexed frame data). The actual browser loaded native
actors, buildings and original interface crops from these packs.

## Panel evidence

The before recording uses unchanged production sources at
`4272cf15583bfb25899f0dd50695504caee0e8ce` in a separate checkout, with only
the opt-in capture test added. The panel take uses the working tree at that
same source HEAD, including the panel and jump-view presentation. `/health`
reports `build: local`; these are source-identified local takes, not signed
or clean-commit executable provenance.

Files in `.cache/showcase/`:

| File | Seconds | Frames viewed directly |
| --- | --- | --- |
| `before-trimmed.webm` | 5.00 | First/middle: native Castle, General and soldiers over original grass; last: existing 14-soldier General token, without the cockpit. |
| `item1-cockpit.webm` | 14.96 | First/middle: native Castle and soldiers; last: original parchment panel, 18/25 population, 32 occupied tiles, held stocks, zero deliveries and 15 military units by type including the General. |

The exact first, middle and final available film frames were extracted with
the pinned `aoeworld/ship-tools:5.1.9` ffmpeg/ffprobe tools and viewed.
Separate `before-{near,far}.png`, `item1-{near,far,last}.png` and capture JSON
retain the browser's screenshots, camera and authoritative HUD observations.
The far diamond is largely black under real early-match fog. Its blue actor
marks are normal strategic markers; the near frames verify native sprites,
not square sprite fallbacks. The demo has no source terrain overview raster.

## Checks and limits

Tests were written before implementation. The panel browser test first failed
at its missing API. Native delivery/report tests and a nonzero current-wire
golden pin verify actual income, expiry, stock-edit exclusion and conversion.
Existing wire pins were regenerated for the deliberately appended player
field; no discriminator was renumbered and no older schema is retained.

Dockerized focused checks passed:

- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(cockpit_deliveries)'`: one test.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(cockpit_player_report)'`: one test.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-protocol AOE_NATIVE_TEST_FILTER='all()'`: 92 tests.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-sdk AOE_NATIVE_TEST_FILTER='test(world::)'`: 28 tests.
- `make test-e2e AOE_E2E_WORKERS=1 AOE_E2E_SPECS=tests/rendering/cockpit.play.spec.ts`: four passed, zero skipped/flaky/unexpected.

The original revision failed the unchanged WASM size gate at 380,214 gzip
bytes (cap 378,521), reproduced in a separate checkout. The owner confirmed
an independent fix was in progress. This branch fast-forwarded to
`85ceef00c` when that fix became available. The integrated slice worktree
then passed `make perf-wasm-cap`: **378,341 bytes**, **180 bytes headroom**;
diagnostics 90,268 bytes. No gate or baseline was changed here.

One startup failure was diagnosed from the headless trace as Chromium
`ERR_NETWORK_CHANGED` during simultaneous Docker job startup. The before
take passed when rerun after that job ended. Other failed attempts and raw
reports remain ignored local evidence; a failed run is not counted as a pass.

These are local demo workloads using ordinary server state/orders and real
art. They do not qualify finished RTS performance, a source-backed large-map
match or dedicated hardware. Full suites, CI and publication were not run.
