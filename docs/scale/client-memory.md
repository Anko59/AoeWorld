# Exploration memory investigation

Workload: a 16,000 × 16,000 tile map, with the owner's report of a Firefox
crash after hours of exploration in a 100-player France match. The browser
regression uses synthetic terrain and transport; it does not reproduce an
hours-long match, qualify a GTX 1660 Ti, or establish finished RTS performance.

## Retention findings

- Server visibility allocated a 4 KiB reference-count array for every chunk
  ever seen and kept it after all counters returned to zero. There are
  250,000 32×32 chunks in this workload: complete exploration could retain
  976.6 MiB of these counters **per player**, excluding tree/allocator overhead.
  This is a server allocation, so it can exhaust shared host RAM without
  proving a browser-local leak caused the reported crash.
- Destroyed buildings unseen by a player stayed in three ghost indexes until
  the footprint was revisited. Repeated destruction could retain unlimited
  historical identities.
- Each overflowing server subscription spawned another task retaining up to
  four full views. A bounded outgoing channel did not bound these tasks.
- The server terrain cache charged decoded binary length while retaining a
  hexadecimal string twice that size. Its nominal 512 MiB budget could retain
  approximately 1 GiB of string payload before other allocations.
- Browser tree-exhaustion timestamps outlived their three-second effect.
- Reconnecting forgot another three browser callbacks, each retaining the
  client. Callback ownership needs to end with its connection.
- Strategic fog encoded a fresh PNG on each changed view, allocating image
  data, encoded strings, and browser image decoding work repeatedly.
- Individual unit deltas were bounded, but their merged client table lacked
  an independent cap. Overflow now requires reconnecting before insertion.

Server counters now live only while their chunk has visible tiles; overlapping
observers share an occupancy count. Ghosts evict in deterministic insertion
order at 4,096, removing every spatial index and notifying consumers. One
subscription worker keeps one in-flight view and one replaceable latest view,
and session replacement cancels it. Tick/resource deltas cannot overtake an
unfinished view. The terrain cache charges retained string capacity and entry
storage against its 512 MiB ceiling.

## Residency bounds

| Owner | Bound / lifetime |
| --- | --- |
| Decoded terrain | 512 chunks and 128 MiB of owned chunk payload; evict most distant, prioritizing visible chunks |
| Chunk requests | 64 live fetches, cancel retired camera demand |
| Height probes | 65,536 entries, FIFO eviction; candidate coordinates also capped at 65,536 |
| Prepared terrain | Current scene and last presented scene, shared through `Rc`; 24,576 projected triangles per scene |
| Game atlas | Eight 2048² RGBA pages (five colour, three player masks), 128 MiB; static throughout exploration |
| Atlas decoding | WebGPU streams one destination page; CPU backends decode each source once into their existing full atlas |
| WebGPU depth and coverage | Viewport-sized Depth24Plus (budgeted at 4 bytes/pixel) and one shared occlusion/hover R8Uint mask (1 byte/pixel) |
| GPU instance buffers | 64 MiB each backend; replaced/destroyed on growth, reused otherwise |
| Canvas backing | 4,194,304 pixels, at most 4096 per axis; raster buffers shrink after large resizes |
| Terrain minimap | One raster, at most 512² RGBA pixels; no terrain chunk requests |
| Strategic fog raster | One 256² canvas, one ImageData and one 65,536-word comparison buffer: 768 KiB payload |
| Subscription fog | One replacement view: at most 512² tile states and 65,536 overview entries |
| Units | 16,384 per subscription; snapshots replace residents and removals prune interpolation |
| Interpolation | Eight samples per unit, at most 16,384 histories |
| Buildings / ghosts received | 16,384 per message and merged building table |
| Wildlife / projectiles | Replacement subscription tables, at most 4,096 each |
| Resource amounts | 65,536 entries; authoritative depletion must survive cosmetic eviction |
| Tree effects | 4,096 timestamps, expire after three seconds even without new resource messages |
| Incoming protocol frame | Binary ArrayBuffer only, at most 4 MiB, checked before the WASM copy |
| Outgoing browser socket | 1 MiB queued payload ceiling; close before sending beyond it |

These are logical owned allocations, not a sum predicting process RSS.
Allocator arenas, WASM linear-memory high-water marks, browser heaps, graphics
drivers, and shared pages require separate observation. Scene accounting can
count the same shared scene twice; tree node/allocator overhead is not counted
by every payload counter. GPU accounting excludes driver and swapchain storage.

## Regressions and measurement

`client_memory_snapshot()` returns on-demand cache/resource counters without
retaining sample history. The Playwright test stores samples outside the tab.
Chromium heap measurements use CDP; Firefox reports an unavailable heap metric
as `null`. Firefox records WASM bytes and `/proc` RSS for the sole game tab's
Web Content process, rejecting missing or ambiguous content processes and PID
changes between sweeps. Its full browser tree is retained as diagnostic evidence.
The instrumented parent can grow independently of content residency, so Firefox
asserts the unchanged 64 MiB plateau across three sweeps without a whole-browser
RSS ceiling. Chromium retains its whole-tree 1 GiB ceiling and 64 MiB plateau;
its RSS sum counts shared pages more than once. GPU bytes describe owned
textures/buffers, not driver residency.

Each sweep records pre-collection RSS, requests page garbage collection, then
samples retained memory. This standardizes collection timing for the plateau
comparison; it does not show that a browser naturally reclaims memory at that
cadence. Chromium's 1 GiB ceiling applies both before and after collection.

At the pinned 1280×720 backing size, Chromium WebGPU reports exactly
138,825,728 texture bytes: 134,217,728 atlas bytes plus 4,608,000 bytes for
depth and coverage. The coverage mask already uses the smallest renderable
integer format, is shared by hover/occlusion, and resizes with the viewport.
The regression pins that exact required total for every sweep; the previous
128 MiB bound allowed only the atlas and omitted both viewport targets. All
other renderer allocation budgets remain 128 MiB.

The native visibility regression touches every one of the 250,000 chunk
addresses three times. Durable explored bits remain; currently visible counter
storage must disappear as the observer leaves. Separate cases cover overlapping
observers and ghost-index eviction.

The browser regression traverses a 12×12 grid spanning the whole map extent,
adding at most 32 midpoints until more than 1,024 distinct chunks have been
visited. It repeats that exact route three times without resetting the client.
It requires more than 512 refetched chunks, checks each cache budget, and bounds
RSS and WASM growth between completed sweeps. This samples distant regions;
it does not fetch all 256 million terrain tiles.
Camera jumps replace the current URL and dispatch the same hash-change input
without adding browser session-history entries. The test checks history length
stays constant, keeping browser navigation retention out of the camera-cache
measurement. Reconnect samples allow two animation frames for retired fetch
callbacks to release before counting references; the reference bound stays exact.

Focused commands:

```text
make test-unit
make test-wasm
AOE_E2E_WORKERS=1 make test-e2e AOE_E2E_SPECS='tests/memory/exploration.spec.ts tests/memory/fog.spec.ts tests/memory/reconnect.spec.ts'
make fmt
make structure-check
make lint
make browser-check
```

For serial execution with one admitted heavy slot (and one browser worker):

```text
NEXTEST_TEST_THREADS=12 make test-client-memory AOE_E2E_SPECS='tests/memory/exploration.spec.ts tests/memory/fog.spec.ts tests/memory/reconnect.spec.ts'
```

This invokes the existing native, full WASM, and focused E2E targets in order;
their checks and deadlines remain in force. An unspecified E2E selection is
rejected, so the target cannot accidentally select the full browser suite.

## Initial browser observations

The initial 64-waypoint route visited only 460 chunks, so it did not exercise
terrain eviction and failed its coverage assertion. It still recorded three
completed sweeps in each engine before the fog raster change:

| Metric | Chromium, sweeps 1 / 2 / 3 | Playwright Firefox, sweeps 1 / 2 / 3 |
| --- | --- | --- |
| Process-tree RSS bytes | 694272000 / 682496000 / 698675200 | 1582649344 / 1770049536 / 1961447424 |
| WASM bytes | 53739520 / 53739520 / 53739520 | 120848384 / 120848384 / 120848384 |
| JS heap bytes | 5463852 / 6448428 / 9031240 | unavailable |

Terrain residency was 460 chunks / 20,981,520 payload bytes in both engines.
Firefox used WebGL2: its GPU atlas and JavaScript restoration copy were each
80 MiB, and GPU buffer payload was 226,240 bytes. Those counters stayed flat.
Its entire browser process tree exceeded the 1 GiB ceiling and grew; this is a
failed bound, not evidence that Firefox residency is fixed. That route also
added browser history entries, which the revised regression prevents.

The separate fog regression observed 4,097 PNG encodings where zero were
required. Its replacement uses fixed buffers and no PNG encoding. Local raw
samples are under `.cache/client-memory/before-fog/`; check logs are under
`.cache/client-memory/`. These initial observations cannot substitute for a
successful run of the stronger eviction route on the final changes.

## Earlier local validation and open failure

The corrected 144-waypoint Firefox route visited 1,035 unique chunks and
refetched 793 chunks. History length stayed at one; no browser page errors
were recorded. Terrain stayed at 512 chunks and WASM stayed at 120,848,384
bytes. Nevertheless, complete browser-tree RSS was 1,832,480,768 /
1,993,703,424 / 2,203,029,504 bytes. Web Content RSS alone was 624,320,512 /
675,700,736 / 699,654,144 bytes. The parent Firefox and WebExtensions processes
also grew. These observations do not identify whether the remaining growth is
application, Playwright, browser, allocator, or graphics-driver retention.
That earlier run failed the whole-browser 1 GiB ceiling and 64 MiB plateau.
The current spec measures the game content process as described above; the
earlier observations alone do not establish its plateau.

In that run Chromium timed out at the unchanged 10-second startup expectation,
before producing a sweep sample. The focused browser check was retried directly.
Fog reuse, twelve reconnects (62 client references every time), and stalled
outgoing-socket handling passed. Raw results are in
`.cache/client-memory/after/`, with the complete serial log at
`.cache/client-memory/memory-gate.log`.

The same serial run passed all 2,985 native tests (18 skipped), doctests, and
the full `make test-wasm` target. `make fmt`, `make fmt-check`,
`make structure-check`, `make test-placement`, `make lint`, `make browser-check`,
`make docs-check`, `make hooks-install`, and `make hooks-check` also passed.
These results qualify the individual fixes, but the requested browser memory
plateau is not proven and the original Firefox crash is not resolved by this
investigation alone.

## Follow-up validation (24 October 2026)

The revised Firefox assertion passes: content-process RSS after collection was
621,408,256 / 632,401,920 / 633,520,128 bytes with the same PID. Its full browser
tree still grew (1,861,558,272 / 2,098,741,248 / 2,207,555,584 bytes), confirming
why that tree is diagnostic rather than the game-tab plateau metric.

WebGPU texture payload stayed exactly 138,825,728 bytes on every sweep. Both
engines visited 1,035 unique chunks, refetched 793, and retained 512 chunks;
WASM memory stayed constant across all three sweeps in each engine.

The focused four-spec run passed 21 cases, including all 18 occlusion/hover
cases and both process-selection cases. Chromium's remaining RSS plateau
assertion failed: 786,280,448 / 855,523,328 / 930,492,416 bytes, a 144,211,968-byte
range against the unchanged 67,108,864-byte limit. JS heap, backing storage,
DOM/listener counts and owned renderer allocations stayed nearly flat.
After collection, diagnostic queries found zero live Response, Request or
AbortController objects and three Promises. The native allocation sampler
returned empty profiles; detailed tracing exceeded the 300-second spec timeout.
Native growth is not attributed to a specific game or instrumentation owner.

Reusing the CDP session, waiting for GPU completion, and moving fixture bodies
to real HTTP did not remove that growth. The unsuccessful GPU/transport/trace
experiments were removed. The Chromium assertion remains in force; the complete
exploration spec is therefore still red. Evidence is retained locally in
`.cache/memory-followup/`, including `filtered-final.json` and `final-results/`.

Native tests (3,088 plus doctests), WASM browser tests (514), and the unchanged
WASM size cap passed. Gameplay gzip size was 301,791 bytes; diagnostics was
84,994 bytes. These results do not qualify natural browser reclamation cadence,
the owner's Firefox/GTX machine, or an hours-long 99-AI match.

## CI response-history correction

The Chromium growth above includes its inspector's retained response bodies.
Playwright enables Network sessions without explicit history limits; Chromium's
InspectorNetworkAgent defaults to 200 MB. Native maps localized the growth to
PartitionAlloc while the application heaps and renderer payload stayed flat.
Changing application fetches to `no-store` still failed the RSS plateau.

The regression now uses the same pinned Linux headless shell and project GPU
flags through the public CDP transport, bounding only inspector response history
to 1 MiB total and 128 KiB per resource. Application requests and response bodies
are unchanged. A real-browser contract retains a complete 512 KiB response in
the page after the inspector's copy is evicted. The original three sweeps,
1 GiB whole-browser ceiling, 64 MiB RSS plateau and five-minute budget remain;
Chromium must explicitly report WebGPU for all three samples.

The corrected control visited 1,035 unique chunks and refetched 793. Post-GC
whole-browser RSS was 680,509,440 / 645,480,448 / 645,201,920 bytes; WASM stayed
at 53,739,520 bytes. This corrects an automation measurement, not the owner's
natural-browser crash or dedicated-hardware qualification.

## Remaining qualification

Durable explored bits and last-known resource amounts are authoritative game
knowledge, not disposable render caches. Full 16,000-square exploration uses
30.5 MiB/player of explored bit payload before container overhead. The merged #566 review fixes page large resource resets, so they no longer
need to fit in one protocol message. This integration retains that publication
contract and updates the new tree-lifetime fixture to supply its snapshot-page
field. Durable knowledge must not be represented as measured browser cache
leaks or silently discarded to obtain a memory plateau.

Real assets, screenshots, and measurements stay outside Git. Repeat the route
on the owner's Firefox/GTX machine and an hours-long 99-AI match before claiming
that the original crash has been reproduced or eliminated.
