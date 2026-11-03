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

## Follow-up: CI run 41612096954

This run tested `15ff318adc158fbecb451a25c378afc7c68b57d3`.
Its original job logs and both E2E JSON artifacts were inspected, rather than
inferring causes from the brief's one-line errors.

- **Unit build:** `static (unit)` exhausted its 4 GiB container cap compiling
  the harness test binary. A plain local `make test-unit` reproduced an OOM
  while compiling simulation tests; neither attempt reached test execution.
  Set `profile.test.debug = 0` for workspace tests. This omits compiler symbols,
  retaining optimization, debug assertions, overflow checks and test selection.
- **Building flow:** CI expected `Villager` but selected `House` at
  `selectWorker(0)` immediately after foundation placement. The helper clicked
  a moving builder's earlier HUD coordinates. This is an interaction flake:
  register the two idle builders in control groups, then recall the intended
  builder during construction. Keep the House selection, stop/resume,
  completion, hit-point, population, Mill, Farm and wall assertions.
- **Single builder's queued House:** CI read `Idle` at the immediate
  `/Moving|Building/` assertion, before queuing the House. Placement sends an
  asynchronous server order, while selection changes can refresh the HUD first.
  Treat this as a synchronization flake: require the authoritative Farm
  foundation, then poll the same builder/task assertion before queuing. A
  persistently idle or unassigned builder still fails, as do lost queued work,
  incomplete construction and incorrect final farm staffing. No simulation
  workaround was introduced.

Before changing either spec, both passed unchanged locally: four cases,
zero unexpected, skipped or flaky results, and no quarantine retries. The
report (`2026-11-03T10:12:45.287Z`) is
`reports/squad-ci/building-farm-before.json`; the original CI reports are
`ci-e2e-3-41612096954.json`, `ci-e2e-4-41612096954.json`, and
`ci-unit-41612096954.log` in that directory. The unsynchronized task read is
a timing-dependent assertion, not evidence that the queued House was lost.

The first complete native run executed 3,786 cases: 3,783 passed, three failed,
and 18 were skipped under existing policy (13 qualification/exhaustive cases
and five explicitly ignored private-asset/source cases).
It exposed a genuine follow-up regression: the formation-only wider-probe
guard in `15ff318ad` also stranded the independent scout in the existing
fractional parking-wedge test. Allow those probes for non-combat traffic as
well as formation members, while retaining the combat pursuit policy and the
formation-only lateral refinement. The original scout, boxed give-up, exact
40-versus-40 melee and spacing/replay assertions all pass (43/43 focused cases).

The harness failure was its unrelated sleep-based playtest/review concurrency
assertion: the one-second child had finished before the test observed the
review. It passes unchanged in isolation (1/1); no harness test was weakened
or unrelated production behavior changed. The third failure was the current
late-game replay pin; its activity floors and independent one/four-worker
hash comparison passed before reaching that assertion. Restoring the scout's
non-combat escape also restores the existing 14,000-unit checkpoint exactly:
the focused replay test passes unchanged (1/1). No golden hash, fixture,
workload duration, activity floor or collision assertion was changed. The
browser task poll uses the existing ten-second assertion budget; no test-case
or construction-completion timeout was increased.

### Final-source browser evidence

The final `make test-e2e` selection of both requested files exits zero:
four cases pass, zero skipped, unexpected or flaky results, and an empty
quarantine journal. It started at `2026-11-03T10:31:36.643Z`; retain the report
as `reports/squad-ci/building-farm-final.json`, the journal alongside it, and
the command log as `browser-final.log`.

An intermediate attempt aborted at worker startup (zero test-body time),
overlapping dependency installation by `browser-check`. Another Make parent
received SIGTERM while its containers remained live; those exact children
were observed to completion (container exit zero, four passes). Their
diagnostics remain in `browser-worker-exit.*`, `browser-parent-terminated.log`
and `browser-child-completed.log`. The final normal Make run above passes
independently; these earlier command failures are not counted as passes.

Final recordings in `reports/squad-ci/`, each checked with
`make video-probe SHIP_VIDEO=<absolute path>`: VP9, 1280x720, 25 fps.

| Recording | Seconds | SHA-256 |
| --- | ---: | --- |
| `building-final.webm` | 140.32 | `f01f58d80759ccccf34d91f7a92539260ea6f4d1a45b7bbf268645bdd7cb69dd` |
| `farm-queue-final.webm` | 90.80 | `370d0f9d318df13e010662e5d7191c2359946e893b57d860f6f7155d4fac0df6` |
| `farm-mixed-final.webm` | 18.12 | `1d290537768fd57ef5eac564406cbcc0fc17d261584614564717b477f07afcff` |
| `farm-staffing-final.webm` | 20.40 | `39ab23b1103a075f800c78f2e581eff814277760f86f7f036a6912a6746eb451` |

The matching screenshots were inspected: completed buildings, a full-health
House with one food worker, and three farmers on separate farms. This is the
supported trial-pack marker fallback, not DE-art qualification. Media probes
establish readable metadata; they are not frame-by-frame video review.
Recordings, screenshots and raw reports remain ignored local artifacts;
this audit commits their paths and hashes without committing game assets.

### Follow-up checks and limits

Dockerized validation commands:

```text
make hooks-install
make hooks-check
make fmt-check
make lint
make structure-check
make docs-check
make browser-check
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(spacing::)|test(a_blocked_scout_escapes)|test(forty_against_forty)'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(late_game_full_population_replays_at_the_profile_checkpoint)'
make test-harness-focused HARNESS_TEST_FILTER=ship::playtest::tests::playtest_is_a_child_make_running_alongside_the_review
make test-e2e AOE_E2E_SPECS='tests/village/buildings.play.spec.ts tests/village/production/farm-staffing.play.spec.ts'
make perf-wasm-cap
NEXTEST_TEST_THREADS=4 make test-unit
make pre-commit
```

The final full native gate exits zero: **3,786/3,786 passed**, three slow,
18 existing exclusions as described above, plus the SDK doctest (1/1).
Four test threads match the four-core CI runner and keep local test execution
from oversubscribing it; the complete workspace selection is unchanged.
This run uses the checked-in test profile, with no debug-symbol environment
override. `reports/squad-ci/unit-final.log` retains the summary and doctests;
its bounded capture truncates earlier per-case PASS lines. The 14,000-unit
checkpoint passes unchanged in this run too (134.686 seconds).

Hooks, formatting, native/WASM lint, structure, documentation, browser
TypeScript/ESLint/Prettier checks and the WASM size cap pass. Optimized gzip
sizes are 375,894 production bytes and 90,268 diagnostics bytes; the existing
302,817-byte baseline and +25% rejection threshold are unchanged.
The corresponding logs, focused results and browser evidence are retained
under `reports/squad-ci/`.

The full E2E suite, separate qualification/exhaustive tiers, private-asset
tests and hardware/performance qualification were not run. Synthetic workload
assertions do not demonstrate finished RTS performance. No shipping, push,
PR update or new remote CI run was performed, as the brief prohibits publishing.
