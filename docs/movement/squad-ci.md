# Squad CI regression audit, 2026-11-03

The CI brief starts at `cd1e4bd25c9f` on `movement/squads`. Merging
`origin/dev` (`2d05cf302554f37b6d16e6bdf14502344314a269`) produced
`b2910e0a5`; this removes the heavy-slot queue before validation.

## Causes and changes

1. `forty_against_forty_melee_strike_count` reproduced with first strike at
   tick 129, 45 early strikers and all 80 total strikers, but survivors `(0, 11)`
   rather than `(0, 10)`. This tuple counts survivors, not protocol rules.
   Corridor recovery had added two wider backward avoidance directions to
   ordinary combat pursuits too. Restricting those directions to formation
   members restores the exact existing fight result. No melee pin changed.
2. `a_permanently_boxed_in_mover_gives_up_and_goes_idle` reproduced with an
   active order after the unchanged wait. The wider avoidance directions and
   new lateral escape at an occupied nearby waypoint let independent movers
   shuffle inside a sealed ring. Restrict that additional lateral escape to
   formation members, including detached recovery members. Ordinary movers
   retain their existing obstruction budget; no timeout or assertion changed.
3. `a_unit_that_breaks_a_rule_is_named_repaired_and_the_rest_arrive` failed on
   `unit.general`: the invalid fixture still used army size 101 after squads
   deliberately raised capacity to 200. Use 201 and also test role counts
   totaling 201. The validator stays unchanged. Existing golden-wire boundary
   coverage accepts 200 and rejects 201; this is no new rule or relaxed check.
4. The unchanged Vassal browser test passed in Firefox WebGL2, Chromium WebGPU,
   Canvas and browser-defaults after the dev merge. The initial local run had
   four passes, zero skips, zero flaky/unexpected results and an empty quarantine
   journal. Its earlier CI failure was not reproduced, so its original cause
   remains unproven; no browser workaround or weakened assertion was added.

## Validation

Dockerized commands (focused selections are not full-suite qualification):

```text
make hooks-install
make hooks-check
make fmt-check
make structure-check
make lint
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(spacing::)|test(group_baseline::)|test(bridge_march::)|test(general)'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-protocol AOE_NATIVE_TEST_FILTER='binary(degrade)|test(general)'
make test-e2e AOE_E2E_SPECS=tests/village/vassal/vassal.play.spec.ts
make pre-commit
```

The protocol selection passes 10/10, including all six degradation tests and
four General protocol tests. The simulation selection passes 134/134 (800
outside the filter), retaining the original melee pins, boxed give-up,
temporary obstruction, deterministic replay, General and bridge assertions.
Hooks installation/check, formatting, structure, native/WASM lint and the
staged-source pre-commit checks pass.

The final-source browser rerun also passes 4/4, with zero skips, flaky or
unexpected results and an empty quarantine journal. Its start time is
`2026-11-03T09:26:49.448Z`; the report is retained at
`reports/squad-ci/vassal.json`, alongside `vassal.quarantine.jsonl`.

Recordings are retained under `reports/squad-ci/`. Each was read with
`make video-probe SHIP_VIDEO=<absolute video path>`: VP9, 800x450, 25 fps.
These probes establish readable media metadata, not frame-by-frame visual QA.

| Recording | Seconds | SHA-256 |
| --- | ---: | --- |
| `vassal-firefox-webgl2.webm` | 26.60 | `0db49e4d446661ada9a45faac582c5061d0946b9c07c8ac49219a9728d4d5459` |
| `vassal-webgpu.webm` | 33.04 | `d45d29d25def395e65cbedaa36b9d0e00e38de7ed55f2cea709922a97b6f4145` |
| `vassal-canvas.webm` | 28.44 | `aa32660b945ea9a0ffc4f0ae7d9c8d466e30ae968bef0ff245de8a50540a4f08` |
| `vassal-browser-defaults.webm` | 70.76 | `49dcc04d72abeaa2979513554dd4125c0b40487a3243b4e013669d348d45cd25` |

No preflight, perf-ci, full E2E, publication or remote CI was run. Synthetic
movement tests and browser recordings do not qualify dedicated hardware or
finished RTS performance. Generated reports and recordings remain outside Git
and container images, as required for original game assets.
