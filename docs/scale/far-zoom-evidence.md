# Far-zoom local evidence, 2026-11-03

Captured executable revision: `34675232f27234ee117784c36c3bc9b29dd6d94c`, including camera
fix `46d1ab167e9cba1aa03edc0406df152a8f8da1a6`. Both commits have the requested
coauthor trailer. The initial evidence commit `462ee2737f0f` changed documentation only.
The dev server's `/health` reported the exact executable revision; the saved
`.cache/showcase/wasm-committed.json` identifies that revision with
`dirty: false` and `result: PASS`.

## Checks

Dockerized Make commands passed locally:

- `make hooks-install hooks-check fmt-check lint structure-check docs-check`
- `make browser-check`
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-core AOE_NATIVE_TEST_FILTER='test(projection)'`: eight tests, including the far-zoom clamp regressions.
- `make test-wasm`: PASS on the clean executable revision; the log reports two supporting tests, 259 client integration tests and 394 client unit tests.
- `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=strategic_armies`, and focused `village::tests`, `vassal` and `resource_state` runs: PASS.
- `make perf-wasm-cap`: optimized client gzip **376,665 bytes**, baseline 302,817 (+24.4%), cap 378,521, **1,856 bytes headroom**. Diagnostic gzip 90,268 bytes.

Focused browser runs used `make test-e2e AOE_E2E_WORKERS=1` with these
`AOE_E2E_SPECS` values, rather than the full suite:

| Specs under `browser/tests/` | Result |
| --- | --- |
| `rendering/strategic.play.spec.ts village/input/performance.play.spec.ts` | 12 passed |
| `rendering/strategic.play.spec.ts village/input/cursors.play.spec.ts rendering/bigview-population.play.spec.ts` | 19 passed |
| `rendering/strategic-capture.play.spec.ts`, mode `general` | 1 passed, 1 intentional project skip |
| Same capture spec, mode `vassal` | 1 passed, 1 intentional project skip |
| Same capture spec, mode `large` | 1 passed, 1 intentional project skip |

The strategic regression covers identity, fog-safe reports, label placement,
historical sightings, current deaths, resource visibility/retention/depletion,
paged snapshot atomicity, layer keys, real selection/orders and return to near
zoom. It runs across WebGPU, WebGL2 and Canvas renderer projects. The real-art
capture intentionally uses only the normal-compositor WebGL2 project.
Tests were written and observed failing before the relevant fixes. No baseline
or gate was reduced. At capture time this checkout retained the old queue harness, so heavy
commands used `AOE_HEAVY_WAIT_MINUTES=5`; the captures preceded the dev merge.
The post-capture integration below drops that setting. No exit-75/76 refusal
was retried.

## Browser captures

All captures use the real local server on port 8153, 1280 x 720, original art
from an ignored local pack. Packages and game assets remain outside Git and
Docker images. The General and economy demonstrations are separate workloads
from the source-backed large-map match.

The first, exact middle and final available frames of each trimmed video were
extracted with ffmpeg and viewed directly. Durations were checked with ffprobe.

| Local video in `.cache/showcase/` | Seconds | Viewed evidence |
| --- | --- | --- |
| `far-zoom.webm` | 8.80 | First: idle General token, 14 soldiers and unit-mix bar. Middle: selected token, normal Captain/Trainer panel, marching state and destination arrow. Last: token displaced along its march; zoom remains 4%. |
| `vassals.webm` | 7.92 | First: Castle domain and card with 20 managed/resident workers, resource jobs, idle count and No Town Center wait. Middle/last: ordinary Castle panel selected, active domain. Extra `vassal-layer-off.png`: F3 removes card and domain; the final film frame restores both. |
| `large-map.webm` | 4.16 | First: pink resource marks beside the home cluster at 2% zoom. Middle: F5 off removes those marks. Last: full-map 0.12% view, with a retained deposit mark; JSON independently records eight discovered resources after the pan. |

The real Paris package is `13445235e81ff032da67e4f01f246b71272d891a71506ccc3a95d3add2cb8de9`, 8,000 x 8,000 tiles. Skirmish status reported 99/99 AI opponents ready in eight hosts; health reported 100 sessions and 499 units. The final scene reported a ready 512-sample raster, one terrain draw, eight resources and 3,686,400 strategic working bytes. The final camera is (4000,4000) at zoom 0.0012. The terrain remains mostly black under the real early-match fog of war.

These are short local observations, not an RTS or dedicated-hardware
performance qualification. `sampleFrames` uses 30 observations after ten
warm-up frames; the browser runs on a shared development machine. General
median/p95 frame time was 16.7/19.7 ms (draw median 0.1 ms, UI 0.3 ms); Vassal
16.7/16.9 ms (draw/UI medians 0.1/0.1 ms). At the large map's 2% home view, median/p95 was 16.7/22.7 ms, UI median 0.2 ms, draw median below the timer's resolution and draw p95 0.2 ms. The first film frame also displays a **864.1 ms transition frame** before the stable sample. The short median does not account for that startup hitch. Health reported two tick deadline misses; this is a cumulative server counter, not an isolated match benchmark.

SHA-256 at capture time (local identity checks, not provenance authentication;
subsequent builds replace the two WASM paths):

```text
ead8453a1efa27aa1f48bd75de6aedc3773e8329dadfd47dacc46a591e8a2a4f  far-zoom.webm
cd9f2e8b758fdd8ab5c06915e5b0891a4eac7f76b2ef14c80403b0bc9f115aa1  vassals.webm
9282ee8c532bee726173fe428dfa2e49d6fa9580833eb0644a9826f428e1eab7  large-map.webm
f237c242f10cf74515b123b2c0e56abdd2cd46ee4ed9f28b38707c452e6ced47  general-capture.json
7e2893241cf40edf179ac82d441ccab42ece9dd88fd4c4c623f00c1c352d53ed  vassal-capture.json
e3c59c02d4094135703158faa686171ed775b619b566d25b003b947c10f70bb2  large-capture.json
5db0824815c1558ce48371b3c1572c42a9a577bca40182504e39e220af968312  web/pkg/aoe_client_bg.wasm
aefa735a205a3b9c3cec563ecc721925c1f87340548ad596e9eab7d75130a63a  reports/perf/optimized.wasm.gz
```

Raw clips, extracted `*-film-{first,middle,last}.png`, capture JSON, focused
browser reports and command logs are retained under `.cache/showcase/` as local
unsanitized evidence. They are ignored, not published, and not embedded in Git.

## Scope and remaining qualification

There is no independent squad feed, road-network feed or Vassal slider/settings
protocol on this dev base. Generals, known bridges and the ordinary Castle
selection hook are used. Worker jobs describe the authorized resident stream,
not a claimed global economic census. Server wait reasons are displayed as
reported. See [strategic play](../strategic-view.md) for controls and bounds.

The large-map match is an early match, not 100,000 units or 1,000 players.
No full e2e suite, performance benchmark, CI run, dedicated-hardware test,
`make ship`, push or publication was performed in this session. CI evidence
for these new commits is unavailable. Synthetic/demo workloads do not prove
finished RTS performance.

Cleanup passed: `/skirmish` reported `phase: stopped` and `ai_hosts: 0`; `make down DEV_PORT=8153` stopped this checkout's server. `make status DEV_PORT=8153` reported stopped and the port-8153 health request failed to connect as expected. All focused browser jobs completed, including their disposable-server cleanup. No test match or task-owned server remains running.

## Integration with current dev

After the initial handoff, stop hook `stop:5` rejected `heavy` in the old
registry: the judge at `2d05cf302554f37b6d16e6bdf14502344314a269` accepts the
current `long` schema. The original failure is preserved in
`.cache/showcase/stop-schema-failure.txt`. Dev was merged, retaining its current
resource-admission implementation and registry together. No parser alias,
gate removal, budget reduction or baseline change was introduced. Subsequent
commands omit `AOE_HEAVY_WAIT_MINUTES`; no actual admission refusal occurred.

Dev's additional proof files exposed a 15-file directory. Moving this task's
helper to `browser/tests/proofs/strategic/large.ts` and updating its two imports
restored the existing 14-file limit. The failing structure log is retained as
`zoom-merge-static.log`; the successful rerun is `zoom-merge-static-green.log`.

Post-merge Dockerized checks:

- `make hooks-install hooks-check fmt-check lint structure-check docs-check browser-check`: PASS.
- `make deny architecture-check browser-check`: PASS.
- `make test-resources`: eight passed.
- `make test-harness-focused HARNESS_TEST_FILTER=gates::registry::`: 35 passed.
- `make test-harness-focused HARNESS_TEST_FILTER=long_`: nine harness unit tests, one resource-runner unit test and four integration tests passed, including immediate long-job refusal and short-job admission fixtures.
- `make test-harness-focused HARNESS_TEST_FILTER=agents::`: 87 passed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(economy::farming)'`: 18 passed.
- `make perf-wasm-cap`: **377,821 gzip bytes**, +24.8%, cap 378,521, **700 bytes headroom**. Diagnostic gzip remains 90,268 bytes.

- `make test-wasm`: PASS; two supporting tests, 264 client integration tests and 394 client unit tests.
- `make test-e2e AOE_E2E_WORKERS=1 AOE_E2E_SPECS='tests/rendering/strategic.play.spec.ts tests/village/input/cursors.play.spec.ts tests/rendering/bigview-population.play.spec.ts tests/camera/hooks.play.spec.ts'`: 29 passed, 0 skipped, zero unexpected or flaky results.

`make gate-run HARNESS_CADENCE=stop HARNESS_SCOPE=working HARNESS_EVIDENCE_DIR=/tmp/aoeworld-zoom-stop-evidence-20261103` also passed all eight stop-cadence gates, including the WASM build. Its bootstrap-local ledger is local, non-authoritative evidence; logs and ledger are copied to `.cache/showcase/zoom-merge-stop/`.

These checks ran on the merged worktree before the integration commit;
the saved `merge-wasm.json` therefore correctly records source HEAD
`462ee2737f0ff637443dab4b56ce2f7457d81e18`, `dirty: true`, and `PASS`.
The earlier films and frame observations remain tied to captured revision
`34675232f272`, rather than being relabelled as captures of the merged build.
The post-merge browser regression verifies the real General selection/march,
resource cache and near-camera behavior. No new original-art film was recorded.

The post-merge logs and JSON copies are retained under `.cache/showcase/` with
`zoom-merge-`, `merge-browser-regression` and `merge-wasm` names. The full e2e suite,
CI and hardware qualifications excluded above remain unrun. No ship, push or
publication was attempted. The dev server stays stopped, and disposable browser
servers and WASM drivers were cleaned up when their checks completed.

Post-merge optimized client gzip SHA-256: `57f95e7d3947e35ef8371d5d7421bffb7ed188b9866700bfc76d8375bd70e927` (`.cache/showcase/merge-optimized.wasm.gz`).

## CI camera regression repair

On 2026-11-03, `git merge origin/dev` from `79c415224ede` reported already
up to date. The native strategic test reproduced the reported failure at its
small-map centre assertion: actual `(512,512)`, expected `(256,256)`.
Its preceding 16K coverage and pointer-anchor assertions passed. The client
already permits outside-map focus; only this obsolete test called the unused
`clamp_center` helper. That helper and its superseded clamp tests are removed.
The strategic regression retains its 16K checks and now additionally requires
small-map coverage and pointer anchoring with an outside-map focus. Core tests
cover outside centres and elevated pointer anchoring across zooms and map edges.

Dockerized checks on the repair worktree:

- `make hooks-install hooks-check fmt-check lint docs-check structure-check browser-check`: PASS. The reported CI static failure did not reproduce locally; remote CI logs were not accessed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='binary(strategic)'`: five passed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-core AOE_NATIVE_TEST_FILTER='test(projection)'`: eight passed.
- `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=wheel_zoom_preserves_finite_nonfinite_and_clamped_pointer_zoom`: one passed in Chrome. Earlier filters using the source filename as a module matched zero tests and provide no evidence.
- `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=playground::game::controls::tests::`: four passed in Chrome, covering wheel anchoring, outside-map panning, pan direction and backing size.
- `make perf-wasm-cap`: PASS, client gzip 377,821 bytes; diagnostics 90,268 bytes. No baseline changed.
- `make test-e2e AOE_E2E_WORKERS=1 AOE_E2E_SPECS=tests/camera/hooks.play.spec.ts`: six passed, zero skipped, unexpected or flaky results (`reports/e2e/filtered.json`).

The focused browser run recorded `.cache/showcase/free-camera-raw.webm`
(SHA-256 `5aff2a0029fa03269adad334fe4d6e7c5880397f3a27b0074fca53f0224eb03a`).
Its `camera-flicker.json` records zero changed pixels out of 400,000 sampled
pixels at each zoom 0.25, 0.5, 1 and 2, with focus `(-2,2)` throughout.
The 0.5 first-frame crop was viewed directly: stable synthetic grass terrain.
This is camera evidence using test assets, not new original-art or RTS
performance qualification. Video, crops and JSON remain ignored local files.
The run used the repair worktree before commit, not a clean committed build.
No full e2e, preflight, ship, push or GitHub CLI command was run.

## Cockpit3: source landscape and first pull-back (2026-11-04)

The `ui/far-cockpit-3` work starts at `205fab7629475894c8426a5dcb92db8cdccd03c3`.
Review fixes are `0f42e41ab86933a001107935b2d92d235388885b`. The following feature
commit contains the landscape and pull-back changes described here. Captures
report the served identity `0f42e41ab-dirty`; they precede that feature commit.
No publication, push or recordings/assets in Git are part of this work.

Both takes use the existing source-backed Paris package
`13445235e81ff032da67e4f01f246b71272d891a71506ccc3a95d3add2cb8de9`, 8,000 × 8,000
tiles, original game art, fog explicitly disabled, and the actual starting
village: four live units and one Town Center. No sockets, terrain, actors or art
are mocked in the capture. This is a new-game landscape inspection, not a
crowded RTS or dedicated-hardware qualification. The developer timing overlay
is visible. Village, pull-back, land, cockpit and whole-world PNGs were inspected.

Ignored local artifacts:

- `.cache/showcase/c3-before.webm` and `c3-before-raw.webm`
- `.cache/showcase/c3-after.webm` and `c3-after-raw.webm`
- `.cache/showcase/c3-before-capture.json` / `c3-after-capture.json`
- `.cache/showcase/c3-final-stress.json` and `c3-final-transition-sheet.png`

The final real-map take samples 30 frames after warm-up. Steady frame interval
median/p95 is 16.7/17.2 ms; moving by bounded keyboard pans is 16.6/21.7 ms.
Moving CPU draw median/p95 is 0.5/0.7 ms. The one-time terrain bake is 31.8 ms.
Nominal terrain backing is 4 MiB, viewport backing 3.52 MiB, and six cached
64² architectural symbols add 96 KiB. Fog, path storage, driver/compositor
allocations, original-art residency, capture overhead and server memory are
additional. The capture uses headless software rendering, not a hardware GPU
elapsed-time measurement; transition and cold-load costs are not those medians.

A separate synthetic stress test supplies 3,000 footprints and accepts at most
2,048, with at most 192 settlement symbols and 30 measured warm frames. Initial
per-frame plot paths and SVG positioning cost about 18–20 ms median. Cached
owner paths and tiny canvas symbols reduce paint median/p95 to 5.1/12.0 ms
(WebGPU project), 3.3/4.9 ms (Canvas), and 4.5/11.8 ms (browser defaults).
Corresponding captured frame p95 values remain 47.5/34.5/53.2 ms. These figures
bound this synthetic kernel and do not establish finished RTS performance.

Functional crop/source tests and 26 selected client WASM tests pass. The broad
browser run had one reveal assertion subsequently corrected and a Canvas
startup failure whose trace shows network-change errors; the final focused browser rerun passes all 33 tests, including both affected
cases, without retries. Static, lint, structure, hooks and WASM-size
checks pass; the optimized WASM gzip is 384,377 bytes against 375,952 baseline
(+2.2%, unchanged +25% gate).

The optional stored-map native dump diagnostic fails its unchanged 30-second
raster-generation assertion: 87.1 seconds in the broad run and 115.9 seconds
when selected alone. Its cause relative to the base revision is unqualified.
The assertion and timing budget were not changed. This remains a cold-raster
qualification limit. No road-network vectors or political territory data exist
in this stream: bridges, actual building plots and active Castle domains are
shown; connections and borders outside those data are not invented.

Commands used for the changed paths (logs are under `.cache/showcase/c3-*`):

```sh
make hooks-install
make hooks-check
make fmt-check
make lint
make structure-check
make perf-wasm-cap
make browser-format
make browser-check
make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=village::tests
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map AOE_NATIVE_TEST_FILTER='test(starts_routes_history)'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(source_land_use)'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(maps::tests::minimap)'
NEXTEST_TEST_THREADS=1 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(dump_stored_minimap)'
make test-e2e AOE_E2E_WORKERS=1 AOE_E2E_SPECS='tests/rendering/strategic.play.spec.ts tests/rendering/cockpit-reveal.play.spec.ts tests/rendering/strategic-capture.play.spec.ts tests/rendering/strategic-transition.play.spec.ts tests/rendering/far-build.play.spec.ts tests/camera/minimap.play.spec.ts'
make test-e2e AOE_E2E_WORKERS=1 AOE_E2E_SPECS='tests/rendering/strategic.play.spec.ts tests/rendering/cockpit-reveal.play.spec.ts tests/rendering/strategic-transition.play.spec.ts tests/rendering/far-build.play.spec.ts tests/camera/minimap.play.spec.ts'
```

The source-backed server was started using:

```sh
AOE_OPEN_MAP_PACKAGE=13445235e81ff032da67e4f01f246b71272d891a71506ccc3a95d3add2cb8de9 AOE_FOG_DISABLED=1 make dev DEV_PORT=8155 AOE_ASSET_PACK=local-assets/packs/54916fcb9c1e43c7a63aa486e3c47824b24ba94c887168ed210517273626bff6
```

Media trimming and frame sheets use the pinned `aoeworld/ship-tools:5.1.9`
container's ffmpeg/ffprobe, with networking disabled. They do not alter game
state or add artwork. No full suites, CI or dedicated GPU qualification have
been run for this branch in this session.
