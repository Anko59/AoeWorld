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

## View-key evidence

`item2-views.webm` is a 32.64-second continuous excerpt from the headless
`cockpit-capture.play.spec.ts` take. First/middle/last extracted film frames
were viewed directly: original Castle/grass/soldiers, the marching General
in far view, and the General in combat after an actual damage alert.
`item2-{recalled,army,alert}.png` retain the individual jump screenshots.
The capture records base `(83,131)`, largest army `(102.5,128.5)` and latest
observed damage `(165.54,130.5)`, preserving 4% zoom. It also asserts saved
position and zoom recall. The take is from the working tree on `a16a38c69`,
including the production form preview, rather than a clean executable build.

The same-fragment bookmark regression was reproduced after using H:
recall previously left the camera at the base because the URL had not changed.
The focused test now exercises that sequence as well as layer-key coexistence,
owned-army selection, newest-alert ordering, form focus and world reset.

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

## Authoritative census and production checks

The completed player report appends a fixed 34-kind census. It includes living
transport passengers, building garrisons and blocked shelter exits; the server
computes it once per tick before filtering each viewer's reports. This replaces
reconstruction from the client's bounded actor stream. The current wire golden
pins were regenerated, with no compatibility path or discriminator changes.

Focused current-census checks: protocol `all()` 92 passed; simulation
`test(unit_census)` two passed; server `test(gameplay::economy::) |
test(gameplay::fanout::)` 11 passed; SDK `test(world::)` 28 passed.
`make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client
AOE_WASM_TEST_FILTER=village::production` passed nine tests. A later
`AOE_WASM_TEST_FILTER=units_here_split` run passed all four split tests.
The combined cockpit/views/production browser selection passed ten tests,
including ordinary real-server production and the three-producer mock split.

The two-Barracks film attempt reached a reduced population cap and honestly
requested only one of six units; its assertion expecting six failed. Later
captures warm licensed art before resetting the raided scenario. Separate E2E
invocations were found to share report paths, so final capture/check runs are
serialized. Concurrent index-check compilation also required refreshing the
root protocol build after a stale artifact omitted the new census field.
Failed attempts remain local evidence and are not reported as passes.

`make perf-smoke` was refused with recipe exit 75 because another worktree held
the long-job slot. No retry loop or benchmark substitution was used. The census
is exercised by the focused fanout tests, including 100 viewers, but this is
not a substitute for performance benchmark qualification.

## Final finish window

The owner requested importing `origin/dev`, stopping size compression work and
committing working production within a further 20 minutes. `git fetch origin`
and `git merge origin/dev` produced `5b286bb2ce1dd537cf9adc184763fbec6e5a2f5f`,
including #672 and #676. The approved baseline update was still absent from
`origin/dev` at the final pre-commit fetch (`e7bd12c53`). No baseline was edited
on this branch. `make perf-wasm-cap` measured **378,651 gzip bytes** and failed
the still-current 378,521-byte cap by 130 bytes. The owner-approved forthcoming
block is approximately 469,940 bytes; its merge remains external work.

After the merge, hooks-install/check, fmt-check, lint, structure-check and
browser-check passed. Focused checks passed: protocol 92, simulation 3, server
11, SDK 28 and production WASM 9. The ten product browser tests passed. The
combined run also contained one opt-in recording failure while waiting for
all three queued units to finish; its report is not a full-run PASS.

The final short production proof orders three Militia, observes the ordinary
queue/rally and ends after the first completion, retaining the remaining queue.
Its local outcome is recorded in `.cache/showcase/short-proof.log`; films and
extracted frames remain in the ignored showcase directory. `shipping-size.log`
and `shipping-{static,protocol,simulation,server,sdk,wasm,browser}.log` retain
commands and outcomes. Full CI, large RTS benchmarks and dedicated hardware
qualification remain unrun. Nothing was pushed or published.
