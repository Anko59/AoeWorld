# Bridge brief audit, 2026-11-02

Incomplete: the shared bootstrap lock prevented the corrected bridge tests
and the capture build from starting. The brief requires stopping a command
after a five-minute heavy-work wait. No movement acceptance claim follows
from this run, and no production changes were committed.

## Source and pending changes

Base: `8ceeb3d31fa60340e7adb47f2c2227f7012f5fc1`, branch
`game/general-march`. The inherited modifications to
`formation/slots.rs` and `movement_step/collision.rs` remain uncommitted.
They add a destination-region flood, wider slot searches and a friendly
squeeze in narrow land passages. Their before/after acceptance comparison
is still required; neither tests nor pins were edited.

Static review found that the new slot search falls back to unchecked
`desired` when its searches fail. Its comment promising that two members
never share a goal therefore needs additional evidence. The region flood
also covers a square of radius 40, rather than limiting traversal to 40
steps as its comment says. These are review concerns, not measured failures.

## Commands and results

- `make hooks-install`: pass.
- `make hooks-check`: pass.
- `make lint`: pass, including native Clippy and WASM test compilation.
- `AOE_HEAVY_WAIT_MINUTES=5 make test-game-focused
  AOE_GAME_TEST_FILTER='test(real_terrain::bridge_march)'`: failed;
  the filter selected zero tests. This is not acceptance evidence.
- Corrected command: `AOE_HEAVY_WAIT_MINUTES=5 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation
  AOE_NATIVE_TEST_FILTER='test(bridge_march::)'`. Cancelled at the shared
  bootstrap lock before tests started, when the capture wait hit its limit.
- An ignored capture checkout was prepared under `.cache/showcase/after`,
  using the same `forest-bridge` fixture and ordinary service ticks. Its
  Dockerized `make build` waited more than five minutes at
  `aoe-slot-bootstrap.lock` and was cancelled. The live holder was another
  checkout's Dockerized harness build. No lock or holder was modified.

Local setup and logs are under `.cache/bridge-audit`. They are uncommitted
capture aids, not validated runtime evidence. The inherited `.config/`
directory was not committed. Per the brief, preflight, perf-ci, the full E2E
suite and publication were not run. No client source was changed.

## Required scenario measurements

| Scenario | Arrived | Stuck | Current-run evidence |
| --- | --- | --- | --- |
| Straight bridge, General + 100 | unmeasured | unmeasured | blocked |
| Wooded diagonal bridge, General + 100 | unmeasured | unmeasured | blocked |
| Three rivers and forest belts, General + 100 | unmeasured | unmeasured | blocked |
| Opposing straight-bridge formations | unmeasured | unmeasured | blocked |
| Opposing wooded-diagonal formations | unmeasured | unmeasured | blocked |

The existing bridge suite has nine cases, including 40-unit formations and
villagers meeting a Knight. Its opposing formations belong to the same
player and travel in opposite directions; they do not establish enemy-army
behavior. All these terrain masks are synthetic typed hydrology and wood
nodes, not real-map or dedicated-hardware qualification.

## Video and resumption

`.cache/showcase/bridge.webm` was not produced. No first, middle or last frame
was available to inspect; there is no claim that footage shows the far bank.
Once the external bootstrap contention clears, run the nine bridge tests
with and without the saved pending edits, then the affected output pins.
Accept changes only if the unchanged assertions hold. Record the requested
close-camera crossing, inspect its three frames and verify the army is on
the far bank in the last frame before closing the brief.
