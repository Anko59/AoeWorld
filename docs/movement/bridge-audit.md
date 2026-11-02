# Bridge brief audit, 2026-11-02

The inherited production edits were measured with and without them and
rejected. The restored implementation passes the nine original bridge cases
and all six affected pins. A new two-General case exposes a larger unresolved
failure: 190 of 202 actors arrive. The requested video remains incomplete.
No gate, acceptance assertion or output pin was lowered or refreshed.

## Source decision

Movement base: `8ceeb3d31fa60340e7adb47f2c2227f7012f5fc1`, branch
`game/general-march`. The inherited edits to `formation/slots.rs` and
`movement_step/collision.rs` added a destination-region flood, wider slot
searches and a friendly squeeze in narrow land passages.

With those edits, all nine existing bridge tests passed. However, the
simulation crate's unchanged 864-test suite finished **858 passed, 6 failed**:

- `mixed_forty_cross_a_forest_gap_and_reform`
- `mixed_forty_march_through_town_streets`
- `mixed_forty_follow_a_river_detour_and_reform`
- `opposing_formations_cross_between_forest_edges`
- `general_hundred_march_a_thousand_tiles_past_rivers_and_forests`
- `late_game_full_population_replays_at_the_profile_checkpoint`

The first five retain their arrival and route-work pins but exceed their
collision-work pins. The last case produced checkpoint
`5943b8ebec03a84cc0d783766b0d3b6bdb37660ee7910ac93ac0538ddcce599f`,
where the unchanged pin requires
`f87130519dcee274080406ae69ad42f5344a67064792a0bc14fd57c8ef4c29fb`.
The two production files were restored to the committed movement base;
the rejected edits are saved under `.cache/bridge-audit/`. They were not
committed. A fresh restored-source run passed all 26 existing selected tests,
including the nine bridge cases and all six affected pins. Its only failure
was the newly added two-General case, described below.

Static review also found an unchecked `desired` fallback in the proposed
slot search. Its region covers a square of radius 40, rather than limiting
traversal to 40 steps as its comment says. These concerns were not used to
replace the executed test comparison.

## Measured scenarios with the rejected edits

These are results of the first successful nine-case focused run, before
adding the two-General test. Every arrival was at its exact assigned slot,
with zero excess speed and zero lost group ticks. `Pending` counts commanded
units not arrived when measurement ended; it is not the maximum stationary
interval, which is reported separately in ticks.

| Synthetic scenario | Arrived / commanded | Pending | Last / ideal ticks | Max still |
| --- | --- | --- | --- | --- |
| Straight bridge, General + 100 | 101 / 101 | 0 | 5561 / 5710 | 41 |
| Straight bridge, mixed formation | 40 / 40 | 0 | 4390 / 4804 | 27 |
| Opposing straight-bridge formations | 40 / 40 | 0 | 7600 / 6961 | 18 |
| Villagers, Knight and militia meeting | 4 / 4 | 0 | 621 / 650 | 6 |
| Wooded diagonal bridge, General + 100 | 101 / 101 | 0 | 8482 / 8382 | 30 |
| Wooded diagonal bridge, mixed formation | 40 / 40 | 0 | 7392 / 7479 | 14 |
| Opposing wooded-diagonal formations | 40 / 40 | 0 | 6701 / 6928 | 47 |
| Three rivers and forest belts, General + 100 | 101 / 101 | 0 | 24354 / 22139 | 27 |
| Three rivers and forest belts, mixed formation | 40 / 40 | 0 | 22213 / 21207 | 16 |

The existing opposing formations belong to the same player and travel in
opposite directions. These cases do not establish enemy-combat behavior.
The masks use synthetic typed hydrology and ordinary wood nodes; they are
not real-map or dedicated-hardware qualification. Bridge tests retain their
original 1.3-times-ideal, exact-slot, membership, speed and 60-tick stall
assertions. Their oscillation counts are observations, not a zero-oscillation
acceptance claim.

## Additional test coverage

`general_opposed.rs` now places two Generals with 100 mixed followers each
on opposite banks of the same wooded diagonal bridge. Both receive ordinary
move orders after observation starts. It asserts each General's own census
and uses the existing crossing acceptance assertions for all 202 actors.
This combines General leadership and opposing traffic in one case, rather
than inferring that combination from separate tests. At the 30,000-tick
bound, **190/202 arrived and 12 remained pending**, with a maximum stationary
interval of 21,739 ticks. No unit exceeded its speed or lost group membership.
This fails the unchanged exact-arrival acceptance assertion.

A small experiment retained a group's friendly passing permission on the
physical deck when members detached for recovery or the group widened. It
improved the result to 200/202, but two remained pending and the maximum still
interval was 23,044 ticks. The full simulation run was 864 passed, one failed:
all pre-existing tests passed, but the new case still failed. The experiment
was reverted and saved as `.cache/bridge-audit/deck-recovery-experiment.patch`.
Its log is `.cache/bridge-audit/deck-recovery-tests.log`. It is not an accepted
production fix. This larger case needs further recovery-planner diagnosis;
the brief permits reporting failures that do not yield to a small fix.

## Fresh restored-source measurements

| Synthetic scenario | Arrived / commanded | Pending | Last / ideal ticks | Max still |
| --- | --- | --- | --- | --- |
| Straight bridge, General + 100 | 101 / 101 | 0 | 5558 / 5710 | 30 |
| Straight bridge, mixed formation | 40 / 40 | 0 | 4359 / 4804 | 22 |
| Opposing straight-bridge formations | 40 / 40 | 0 | 7600 / 6961 | 18 |
| Villagers, Knight and militia meeting | 4 / 4 | 0 | 621 / 650 | 6 |
| Wooded diagonal bridge, General + 100 | 101 / 101 | 0 | 8491 / 8382 | 30 |
| Wooded diagonal bridge, mixed formation | 40 / 40 | 0 | 7391 / 7479 | 14 |
| Opposing wooded-diagonal formations | 40 / 40 | 0 | 6688 / 6928 | 47 |
| Three rivers and forest belts, General + 100 | 101 / 101 | 0 | 24198 / 22139 | 28 |
| Three rivers and forest belts, mixed formation | 40 / 40 | 0 | 22239 / 21207 | 28 |
| Two Generals and 200 followers, wooded diagonal | 190 / 202 | 12 | 30000 / 9976 | 21739 |

## Commands and evidence

- `AOE_HEAVY_WAIT_MINUTES=5 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation
  AOE_NATIVE_TEST_FILTER='test(bridge_march::)'`: 9/9 passed with the pending
  production edits; admitted after 153 seconds.
- The same target with `AOE_NATIVE_TEST_FILTER='all()'`: 858/864 passed,
  six unchanged regressions failed; admitted after 92 seconds.
- Restored source: the same target with
  `AOE_NATIVE_TEST_FILTER='test(real_terrain_formations::)|test(late_game_full_population_replays_at_the_profile_checkpoint)'`:
  26/27 passed; only the new 202-actor case failed. Log:
  `.cache/bridge-audit/restored-tests.log`. Admission took 83 seconds; the
  machine-load wait cleared before the five-minute limit.
- Logs: `.cache/bridge-audit/pending-tests-current.log` and
  `.cache/bridge-audit/pending-pins.log`. Earlier incorrect filter attempts
  selected zero tests and are not acceptance evidence.
- `make fmt` formatted the inherited edits and the new test code. The
  restored production code is byte-identical to the committed movement base.
- `make lint`, `make hooks-install` and `make hooks-check` passed; their
  latest logs are under `.cache/bridge-audit/`.
  The prior audit commit `6f019b92` passed its isolated-index `make pre-commit`.

Per the brief, preflight, perf-ci, the full E2E suite and publication were
not run. No client source changed. The inherited `.config/` was not committed.

## Capture blocker and remaining work

An ignored capture adapter under `.cache/showcase/after` uses the committed
movement implementation, the `forest-bridge` fixture and accelerated ordinary
service ticks. Its bootstrap compiled, but Dockerized `make build` failed:
`no free slot after 5 min (AOE_HEAVY_WAIT_MINUTES)`. The reported holders were
`test-harness-focused` in `AoeWorld-sccache` and `perf-wasm-cap` in
`AoeWorld-fastcommit`. The log is
`.cache/bridge-audit/capture-build-current.log`. No holder or gate was changed
to obtain admission, and the failed command was not restarted in a loop.

After the recovery experiment, an intermediate capture build completed with
that experimental code. Its binary was saved locally, and the experiment was
reverted in both the main and capture checkouts. The final recording build
therefore uses the restored production implementation. It was admitted after
238 seconds, then remained at the machine-load gate (21.4 load, 12 cores).
Its wrapper started at 20:07:41 UTC and was still waiting without a build
child at 20:12:51 UTC. It was stopped at that point under the brief's
five-minute total waiting limit. The log is
`.cache/bridge-audit/capture-build-baseline.log`. This repeated the shared
machine-capacity blocker across three goal attempts; further heavy work
requires external contention to clear.

`.cache/showcase/bridge.webm` was not produced. No first, middle or last frame
was available to inspect, so there is no claim that footage shows the far bank.

The remaining delivery is the requested close-camera diagonal, forest-lined
crossing video, at most 60 seconds. Inspect the first, middle and last frames
and confirm that the last shows the army on the far bank before closing the
brief. The larger two-General deadlock is reported as an unresolved limit,
not as successful crossing evidence.
