# Combat playtest repair

Base: `80ff4af26d2a` (`play/tonight`), branch `fix/combat-autoattack`.
Movement, body collision, spacing and formations remain separate work.

## Verified causes

Idle military acquisition was absent. The new idle-priority and AI-defence
regressions failed before implementation. Retaliation required the attacker
to be a unit; Tower/Castle damage therefore did not activate an idle soldier.
The building-source regression reproduced that omission.

Castle right-clicks used the ordinary production-rally command because a
Castle trains a Vassal. Simulation accepted that rally, but wire validation
explicitly forbade Castle rallies. The regression failed at snapshot encoding
with `InvalidEconomyState`. Reconnecting rebuilt the same invalid snapshot.
The old transport flattened encoding errors, socket errors and timeouts into
one warning. Consequently the playtest log's wording did not establish socket
timeouts. Several owner-view sessions and a reconnect now exercise this path
over real WebSockets, alongside a deliberately blocked session and an invalid
attack order.

One tick logged 300,547 us of combat and 291,088 us in the inclusive hierarchy
counter, which spans all tick phases. Hierarchy is a plausible combat hotspot,
not an exact per-combat attribution from that counter alone. The former
firing-position fallback allowed sixteen proofs of up to 8,192
hierarchy expansions per search. Orders ran those searches synchronously on
the async runtime, and repeating an unchanged attack repeated the search.
This identifies costly code and the logged phase, without reconstructing every
order or the unrecorded full match state.

Another logged tick applied 14 orders among 8,276 units and spent 391,508 us
in orders, with 370,723 us in the inclusive hierarchy counter. The log does
not identify every order's kind, so it cannot assign that entire spike to
attack commands.

## Behaviour and confidence

The [Age of Kings manual, combat stances, pp. 41–42](https://manuals.plus/m/bede02f48ca7b2aae252168379555520636b871dcd6c6fdba8ba2766ea0186e2.pdf#page=45)
documents sight acquisition, unlimited Aggressive pursuit, bounded Defensive
pursuit, Stand Ground and No Attack. These broad rules have high confidence.
This implementation uses a five-tile Defensive movement leash plus weapon
range, returning to the acquisition origin; that exact distance is an explicit
approximation, since the manual specifies only a short pursuit.

Most military units default to Aggressive; Rams default to Defensive. No Attack
suppresses acquisition and retaliation; explicit attacks still work. Plain
movement orders retain priority. Idle villagers retaliate against melee units
and escape ranged/building fire while keeping their carried resources.
Working or explicitly moving villagers keep their jobs/orders. Noncombat units
escape when idle. These class reactions have medium confidence relative to all
DE edge cases. AI and human ownership use the same simulation rules; the SDK
already publishes owned-unit/building `UnderAttack` events.

An arrived formation can acquire enemies and releases through the existing
combat/stop API; an unfinished formation move retains priority. Enemy clicks
with a defensive building selected now command its weapon; other clicks retain
the production rally behaviour.

Selection prefers military/support units, then civilian units, then firing
buildings and other structures. Walls, gates, bridges and wildlife require
explicit targets. Rams automatically select siege units and buildings.
Class ordering has medium confidence; no complete DAT target-selection policy
is represented in the repository. Stable distance/entity-ID ties and bounded
sampling are this engine's deterministic policy. A dense stationary crowd gets
a complete rotating unit sweep before building fallback; arriving units can
interrupt an automatic building attack. Explicit player targets remain fixed.

Castle/Tower/TC rates and firepower retain the licensed numeric-table rules:
2 s reload; Castle five base arrows at 11 pierce/range 1–8, Tower one at
5/range 1–8, garrisoned TC at 5/range 6. Garrison limits remain 21/5/10.
Research, projectile flight, visibility and armour still apply. Confidence in
these existing table values is high; this work does not implement every DE
secondary-projectile bonus or accuracy rule.

## Work bounds and evidence

Acquisition considers at most 128 military identities per tick and reserves
part of the shared 16,384 unit-inspection budget for buildings. Each spatial
bucket contributes at most 128 candidates per query. Building scans rotate over
at most 64 defensive buildings. A per-actor window prevents cadence aliasing.
Target acquisition can therefore be delayed in unusually large/dense armies;
it does not perform all-pairs searches.

Pursuit/accepted order searches share sixteen admissions per simulation tick.
Local searches retain/expand at most 1,024 tiles; far fallback uses at most four
256-expansion hierarchy proofs. Over-budget valid orders defer pursuit rather
than blocking admission. An unreachable initial validation remains atomic;
failed validations are bounded per command and by server admission. Unchanged
targets retain routes and cooldowns. Full routing stays with the existing
budgeted movement executor. Exhausted pursuit admission resumes above the
last admitted identity on the next tick; damage resolution retains its canonical
order. A moving-target regression reproduced only 92 of 128 accepted pursuers
receiving routes in 40 ticks before this fair rotation.

The focused benchmark uses 500 Archers per side, 400 ticks, adjacent and
12-tile approaches with fog disabled, 32 repeat attempts per tick while both
members of a pair live, and a two-way-hit
assertion. A separate 1,000-unit/32-tick workload changes 32 far attack targets
per tick; it measures order/pursuit pressure, not completed fighting. Raw phase
samples are written under ignored `reports/perf/combat-*.txt`.
The stationary workload requires damage on both sides of all 500 pairs; the
two 400-tick layouts record engagement counts and hits alongside elapsed times.

Live `scale-match` reports now retain orders, combat and inclusive hierarchy
phase quantiles. `--browser --record-video` saves the browser recording beside
its ignored JSON report, without changing the default frame-rate workload.

The initial militia approach trial did not reach contact within its sampling
window and is excluded from combat conclusions. Synthetic/shared-machine
timings do not qualify finished RTS performance or dedicated hardware.

Current replay pins must change because stance, acquisition cursors, target
windows, explicit building targets and pursuit scheduling enter authoritative
state, while automatic combat changes emitted units. Regenerate only current
checkpoints and wire streams, retaining the scenarios and independent replay,
visibility and thread comparisons. No performance threshold or gate changes.

## Paired benchmark results

Base `80ff4af26d2ad0052b91c419bc187e7e959a15c2` ran with only the benchmark
module and its registration added. Candidate production code was preserved in
Git tree `c92aa222c13a3d3da3816a8daa62dc0f0b9aed8e` and restored afterward.
Both used the normal `target/`, test-profile optimizations, the same 150-line
fixture (SHA-256 `9d147ec64605df492b61e61bb14b00e069530b777b353e2fde588a295e8e708f`)
and four-CPU heavy slots. Raw samples: `reports/perf/combat-before/` and
`reports/perf/combat-after/`; summary: `reports/perf/combat-comparison.json`.

Each cell below is **base → candidate p95, milliseconds**.

| 1,000-unit workload | Combat phase | Orders | Orders + simulation |
| --- | ---: | ---: | ---: |
| Adjacent, 400 ticks | 0.139 → 0.239 | 0.008 → 0.006 | 0.174 → 0.279 |
| 12-tile approach, 400 ticks | 0.161 → 0.625 | 0.424 → 0.008 | 1.162 → 1.503 |
| 32 changed far orders/tick, 32 ticks | 0.123 → 5.316 | 345.236 → 19.606 | 345.942 → 24.749 |

Both armies fought in all 500 pairs in both layouts and revisions. Adjacent
hits were 8,000 in each; approach hits were 8,000 → 7,400. New acquisition and
deferred routing change combat timing and outcomes. Initial approach command
setup was 11.099 → 1.146 ms. Far-pressure p95 including simulation falls below
the 50 ms budget; some search work moves into combat and valid pursuers wait
for admission. The separate moving-target test checks their progress.

These are single shared-host batches, not dedicated-hardware measurements.
Candidate samples came from the focused combat run; base ran the two benchmark
tests. Times exclude server fan-out, rendering and Python decision work, and
the far workload never reaches fighting. The added combat work increases the
small close/approach phase costs; no universal performance claim is made.

## Validation and remaining qualification

Dockerized checks completed during implementation:

- `make hooks-install hooks-check`, `make fmt`, `make structure-check`,
  `make lint`, `make architecture-check`, and `make browser-check`: passed.
- Focused simulation combat: 105 tests, including both 1,000-unit benchmarks
  and all 128 moving pursuers progressing. The final run required regenerating
  the current replay checkpoints for the authoritative changes described above.
- Focused server transport/tick: multi-session Castle rally/attack, rejected
  target, reconnect, blocked session, and distinct outgoing failures passed.
  Current wire stream checkpoints were regenerated for the new stance field.
- `make test-wasm`: passed (2, 209 and 359 browser tests in its test batches).
- `make test-e2e AOE_E2E_WORKERS=1` with
  `AOE_E2E_SPECS='tests/memory/error-banner.spec.ts tests/village/military/combat.play.spec.ts'`:
  passed; three tests. The separate focused
  `tests/village/military/defensive-attack.play.spec.ts` also passed. These cover
  error/reconnect reasons, the reload link, visible authoritative stances,
  Castle rally reload and enemy-click weapon dispatch.
- Focused harness scale tests passed the new phase quantiles and recording
  flag validation. Current scale replay checkpoints were regenerated.

The first final preflight stopped before native tests on new production
`unwrap()` calls; both were replaced with checked handling. The second final
`AOE_TEST_ALL=1 make preflight` is pending at this report's preparation.

The exact original 100-session incident, original match-state replay, real-map
AI combat with a watched browser recording, Python integration suite, and
dedicated-hardware performance qualification are not established by the
focused evidence. Video capture support has unit/configuration coverage;
an actual recording has not yet been exercised. No CI publication was run.
