# Large-map static navigation

Measured on the shared Linux development machine, 25–26 October 2026 (Paris).
This is synthetic navigation evidence. It does not qualify the owner's live
France match, total France RSS, finished RTS performance or dedicated hardware.

## Root cause reproduction

Production baseline: `30c247eb6ef59627d84ec811423ba175320aa1b7`, with only
test instrumentation added. `make test-navigation-perf` ran the same workload
with lazy navigation and the existing heap snapshot before storage changed.

The workload has a 2,048 x 2,048 tile map (4,096 chunks), 7,000 scouts owned by
100 player IDs, distributed over a 10 x 10 grid. It issues 20 deterministic
distant move orders before each of 160 measured ticks. Fog is disabled; there
are no villages, economy, fights, clients or server fan-out. A one-sample dry,
flat prepared environment avoids original game assets and geographic downloads.
The reduced map exercises the same lazy and complete-static lookup paths as a
France-size map. The separate storage test exercises all 250,000 index entries.

Release measurements, milliseconds (p50 / p95):

| Phase | Lazy baseline | Existing heap snapshot |
| --- | ---: | ---: |
| Whole advance | 20.032 / 47.056 | 4.652 / 6.327 |
| Movement | 19.822 / 47.026 | 4.486 / 6.131 |
| Planning, nested in movement | 15.511 / 46.247 | 2.120 / 5.139 |

Both modes produced replay hash
`e1c1ffdddd4a25dc6d4bc9958092bddb7600a546d4b13752b20af5142adf6223`.
The test pins that current result for both storage modes.

The lazy measured loop, including order admission, recorded 420,113 terrain
point queries (3,601 ms inclusive), 88 walk chunk builds and 83 portal builds.
The static mode recorded zero terrain point, walk, portal and portal-cost builds.
The paired result confirms that the lazy terrain/navigation path dominates this
workload. It does **not** demonstrate cache thrashing: only 88 walk and 83 portal
entries were retained. It also does not isolate the live match's component
resolution workload. Navigation spans overlap; their times must not be added.

Raw local summaries and baseline output are in ignored
`reports/navigation/lazy-before.json`, `eager-before.json` and `before.log`.
Shared scheduling makes elapsed timings informational.

## Storage and preparation

The current navigation format is 2; there is no old reader. Files retain the
package hash, versioned filename and native package lock. Each index record
contains an offset, length and BLAKE3 checksum. Installation verifies identity,
dimensions, complete coverage, non-overlapping payloads, checksums and chunk
structure before exposing a table.

Chunk records use explicit little-endian fields: six-byte walk runs, 16-byte
resource nodes with implied tile coordinates, eight-byte portals with local
coordinates, component-label runs and flat u32 portal costs. The maximum u32
marks unreachable pairs. Costs retain their exact range; there is no u16 cost
quantization. Every resource ID, amount, kind, object and variant is preserved.

The architecture gate forbids first-party unchecked mapping and production
`expect` calls. The server therefore uses memmap2 0.9.12's safe `map_if_safe`
API. Kernel-immutable fs-verity files map directly. On Linux, ordinary cache
files stream through a 1 MiB buffer into a memfd; write, grow, shrink and further
seal changes are forbidden before mapping. Kernel sealing proves immutability,
including if the original unsealed disk file is later truncated. No gate was
changed or bypassed. The pure map crate reads byte views without OS APIs.

The compact table remains capped at 2 GiB. A sealed memfd is shared across
worlds in one process; separate processes have separate sealed copies. Its
pages can be swapped, but cannot be evicted to the original disk cache file.
fs-verity mappings have ordinary shared disk-page eviction. Clean source file
cache can coexist transiently with the sealed copy and is reclaimable; the
reader uses one bounded buffer. Off-tick loading is serialized before rechecking
the snapshot registry so simultaneous worlds do not create duplicate copies.
This uses the bounded compact-encoding alternative when direct safe file mapping
is unavailable. Ordinary-cache sealed mapping is qualified on Linux; unsupported safe loads
report an error rather than using an unchecked mapping.

The publisher still syncs and renames private staging files and syncs the parent
directory. Published files remain read-only; regeneration replaces the path.
The [memmap2 API](https://docs.rs/memmap2/latest/memmap2/struct.MmapOptions.html#method.map_if_safe)
and its kernel immutability check are used without first-party unchecked code.

Preparation streams completed chunks from at most four workers through a
channel of at most eight encoded records to one writer. No whole-map chunk
vector is retained. Each worker has its own bounded 64 MiB walk cache; an encoded
record is limited to 4 MiB. Writer buffering is 1 MiB. Source residency, temporary
local graphs, allocator slack and thread stacks are additional memory.

A published file is capped at 2 GiB. Retained offset entries are capped at
16 MiB; validation temporarily makes one additional bounded copy to check
coverage. France's 250,000 entries occupy 4,000,000 bytes. Queries read the mapped records directly; backing and sharing semantics are
described above. Health snapshot
bytes count the file plus index, not RSS. The process snapshot LRU still caps
retention at 1 GiB of logical bytes and 256 handles.

Package jobs bake off tick. Matches load existing tables off tick. Missing or
running bakes leave lazy navigation available; a bounded background queue
installs the completed snapshot through weak world targets. One background bake
runs per process, with at most eight package requests and 256 targets per
package. A retired world does not retain its caches through an installer.
Maps exceeding the index budget stay lazy without area-sized allocation.

Installed static walk, component, portal and cost data is never constructed in
ticks. Local route searches and overlay-specific graph rebuilding still execute
under the existing deterministic planner budgets, using stored walk data.

## Regression coverage

New tests compare mapped and lazy walk fields, components, exact route costs,
resources and dynamic collision, including bridges and unavailable source
providers covered by the existing static-navigation suite. A probe rejects any
static terrain, portal or portal-cost construction after installation.

Persistence tests cover concurrent preparation and sharing, checksums and
identity, malformed/partial/duplicate records, cancellation and staging cleanup,
storage-budget refusal, non-blocking lazy queries under a held bake lock,
background sharing after a world retires, and 250,000-record retained heap.

Focused map validation:

```
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map \
  AOE_NATIVE_TEST_FILTER='test(navigation::hierarchy)'
```

Result: 35 passed. The server-focused command was:

```
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server \
  AOE_NATIVE_TEST_FILTER='test(map_store::navigation)'
```

Result after the safe-mapping correction: **16 passed**, including the
250,000-record test (48.1 seconds), source-file truncation, and cancellation
after the 16,000-tile preparation path enters streaming. The stress fixture has
870 walk runs per chunk, zero resources and zero portals; resource packing and
portal/component correctness have separate tests. It does not represent France.

| Synthetic 250,000-record storage | Bytes |
| --- | ---: |
| Persisted compact file | 1,786,000,044 |
| Retained snapshot heap | 4,000,072 |
| Logical snapshot data plus index | 1,790,000,100 |

Linux peak RSS was 1,760,172 KiB (about **1.68 GiB**), including the sealed mapped
pages touched during validation. Clean source-file cache may coexist outside
process RSS and is reclaimable. These are retained-heap and synthetic process
measurements, not peak bake scratch or the total RSS of a populated France
server. The test deletes its file. A dense resource test preserves all seven
object kinds and all four resource kinds within the unchanged 24 KiB per-chunk
allocation assertion; 23 million packed resource records would occupy 368 MB,
plus walk and graph data and index overhead.

`make fmt`, `make structure-check`, `make lint`, `make deny`,
`make hooks-install` and `make hooks-check` passed. The staged commit hook
rejected the initial unchecked map and two decoder `expect` calls; the safe
API and direct endian reads corrected them, and `make architecture-check` passed. Lint initially rejected two
single-range test vector literals; explicit iterators fixed that warning without
changing the assertions. Structure ran once; formatting was repeated after the architecture-gate
correction. Remaining checks use `CARGO_INCREMENTAL=0` in the normal `target/`; the completed 6.7 GiB
incremental cache was removed to preserve shared disk headroom.

## Final release measurement and checks

Implementation revision: **`54994f7e99e374ffeb8babb084a945cac0785a61`**, signed
by Anko59. The source tree was clean for the full native and final release runs.
This report's final update changes documentation only.

```
CARGO_INCREMENTAL=0 make test-unit
CARGO_INCREMENTAL=0 make test-navigation-perf
```

The first native attempt exceeded the unchanged 600-second deadline after
reporting progress at 2,520/3,344 tests; the harness reported load 19.5 on 12
cores. The authorized timeout retry completed **3,344/3,344 tests**, with 18
skipped under the unchanged native policy, then all workspace doctests passed
(one executable doctest; the other libraries had none). The completed Nextest
run took 404.855 seconds. No completed full suite was repeated. Its logs are
`reports/navigation/test-unit.log` and `test-unit-retry.log`.

The focused release target passed both tests on that revision. It uses the
same workload described above and records revision and profile in its JSON.
Times are milliseconds, p50 / p95:

| Phase | Original lazy baseline | Fresh paired lazy | Final mapped |
| --- | ---: | ---: | ---: |
| Whole advance | 20.032 / 47.056 | 18.024 / 42.069 | **4.682 / 6.210** |
| Movement | 19.822 / 47.026 | 17.909 / 42.027 | **4.478 / 5.952** |
| Planning, nested | 15.511 / 46.247 | 15.056 / 41.369 | **2.278 / 5.392** |

The fresh pair reduces median movement time by about 75% and median planning
by about 85%. All runs have the same pinned replay hash. The mapped loop and
advance both recorded **zero** terrain point queries, walk chunk builds, portal
builds and portal-cost builds. Local search continues: these are not zero-work
ticks. The fresh lazy advance recorded 210,411 terrain point queries, 88 walk
builds and 83 portal builds; its full measured loop, including orders, recorded
420,113 terrain point queries. Component resolution was not exercised by this
open-ground timing workload; its correctness has separate mapped/lazy tests.

The 4,096-chunk mapped snapshot retained 752,772 logical bytes, versus 2,252,800
for the original heap control. In the final sequential benchmark process,
mapped-run RSS was 105,284 KiB and peak RSS 178,004 KiB, including bake scratch
and allocator retention. This small-map process measurement is separate from
the 250,000-record storage test above.

Raw final artifacts are `reports/navigation/after.log`, `lazy.json`, `static.json`
and `storage.json`. Shared-host timing differences between the original and
fresh lazy runs are visible; these numbers are informational, not hardware gates.

Final validation includes the staged hook's format, structure, test placement,
architecture, documentation, native Clippy and WASM test-target compilation
checks. `make deny` passed advisories, bans, licenses and sources. No assertion,
budget, gate or baseline was relaxed. The working tree uses its normal target
cache; obsolete binaries/archives from the earlier incremental profile were
removed to restore the native gate's 15 GiB disk floor. No extra target cache
was created.

Per the task instructions, preflight, full E2E, showcase, SDK-Python tests and
publication were not run. There is no CI evidence for these local commits.

## Qualification limits

The owner server and other worktrees' France packages were not accessed. The
storage fixture repeats synthetic chunk contents; it is not geographic terrain
or proof of real France connectivity. Logical byte budgets and synthetic RSS do
not establish the entire real server's memory use with 100 initialized villages,
fog, economy, overlays, sessions and source residency. That live qualification
remains necessary. No performance gate or baseline was relaxed.
