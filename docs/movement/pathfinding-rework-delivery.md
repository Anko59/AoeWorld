# Movement rework: delivery

Companion to [the rework plan](pathfinding-rework.md), version 4. Effort is in agent-days for one
focused agent *(est.)*, review rounds included, owner playtest latency excluded.

## 1. Rules for every phase

- **Shippable alone, no second system.** Rollback is a revert, never a flag. Each phase leaves the
  game better if work stops after it.
- **Gates.** Each PR deletes something, carries a scenario test that fails before it, and runs:
  - the focused gate;
  - `make preflight`;
  - `make test-e2e` for client changes;
  - an in-game before/after video of its own change.
- **Kill rules.**
  - A gate missed by more than 1.25×, or a phase over 1.5× its days, stops work for a re-plan.
  - Zero gates such as "0 waiting units" are binary.
- **Cost defence.** From phase 1b on, every phase gates instructions per full mover and Callgrind
  LL misses × 64 B (DRAM bytes) on a fixed stage set (kernel, apply, index, vision edges) at
  ≤ 1.5× the S1 figures. Phases 1 and 2 gate on no regression against phase 0.
- **Behaviour comes before cost.** Avoidance lands on today's routes before the cutover, so the
  cutover plugs fields into a known-good step. Avoidance also fixes the most visible complaints
  first.
- **During the cutover:**
  - other movement PRs freeze, apart from an owner hotfix lane;
  - merge to dev at least every 3 days;
  - order the PRs so each one deletes something.
- **Tests.** Scenario tests reproducing the owner's bugs stay. Mechanism tests go with their
  mechanism.

## 2. Phases

| # | Phase | Content | Deletes | Gate (this machine) | Days |
|---|---|---|---|---|---|
| 0 | **Truth and harness** | Command recorder. Closed-loop deterministic in-process match at 100 and 1,000 seats: Steward, General and Vassal AIs, a minimal Rust bot tier, with `MAX_ECONOMY_PLAYERS` raised at the simulation level. Per-stage counters (instructions, cycles, L3 misses, DRAM bytes) plus STREAM. Calm share by density. France table encoder census, heights included. Complete cap inventory and semantic lint. ext4 verity check. Owner-versus-dev gap. AoE2 DE reference clips of the S1–S9 scenes where possible | — | Counts ±3% over 3 runs; equal hashes at 1/2/6/12 threads over 30 min; byte budgets committed; R1/R3 lines revised from the census | 8 |
| S1 | **Kernel spike** (a bench written against phase 1's SoA types; promoted or deleted) | Real France terrain and line-of-sight radii; 1M units; frames with members; villager commuters; animals; L2 searches; all-neighbour separation with the half-gap rule; sleeper wakes; vision edges; analytic movers with time-to-contact wakes; player events. Calm share swept at 30/60/80% at R1 density | — | ≤ 2,000 instructions per full mover; movement work ≤ 120 core-ms at 60% calm and ≤ 200 at 30%; serial ≤ 10 ms; LOD on/off identical over 2,000 ticks; RSS ≤ 1.5 GiB excluding the table; wakes per mover-tick reported; wall time pinned to 6 physical cores, median of 3. **Kill at 1.25×:** the owner resets R1 | 7 |
| 1 | **Bookkeeping** (ids stay monotonic) | Incremental flat cell index (Gauss-Seidel preserved); all-unit presence index; mover bitset and dirty bits; chunk subscribers for map changes; vision edge counters; event-driven building memory, gates, transports and `apply_watch`; fog deltas; dirty session diffs; AI query API; ordered maps off the per-mover path | Collision `HashMap`, `chunks` BTreeMap on the hot path, `active_movers` memmoves, `collect_changed` sort, `Tables::refresh`, full `UnitFrame`, all-unit and all-building scans | Hashes identical except one hash-format repin PR; forbidden-pass counters at 0; bookkeeping instructions −50% at the #735 load; RSS per unit −30% | 10 |
| 2 | **Avoidance and parallel apply** (on today's routes) | Hardness table with every row; half-gap rule on previous-tick buffers; sleeper wakes; pair side memory slots; crossing and left-fallback rules; alliance-only narrow-portal arbitration with frame leases; canonical claim merge; restored behaviours with termination; radix and Jacobi apply (the one grid-order repin, with the tick-counter move); analytic predicate and LOD oracle | Hard shares, soft-pass timers, single-file and lane rules, work squeeze, sidestep, yields, idle push, second sweep | B6; deadlock suite; 0 reversals on the 1,000-tile General march; villagers pass in a 1-tile gap; transport passes fishing ship; serial ≤ 10%; LOD oracle on all replays; **R0 ≥ 20 ticks/s pinned at the #735 load** | 10 |
| 3 | **Regions and reachability** (a pure layer over today's passability) | `u16` regions per bucket, canonical union-find, gated portals for reachability, frontier goals | Pocket proof, certified flood, unreachable memo, sealed-ground deadlines | 0 probe ends at unreachable or invalid goals; union-find ≤ 2 ms under churn; enemy ordered into a walled base settles with 0 pressers after 30 s; 1,000 gate toggles cause 0 relabels | 7 |
| 4 | **Cutover** | Overlay grid (cliffs, analytic decks, gates), compressed L1 + dynamic bitmaps (caps deleted), L2 + ALT, goal-rooted refcounted corridors, re-search and escalation rules, fields and pre-pass, lookahead steering, exact sweep, admission on the tick, server command caps, embark graph with fords, detach/resume/leader routes replaced, evictable table mapping and cap lift | Scheduler, planner, routes, lookahead, refinement, escape, deck branches, `detach.rs`, `resume.rs`, planner hashing, every world cap | 0 movers without a direction on every tick; B1, B2, B5, B9 (transport); owner's 11-03 and #727 scenes; cache oracle; ≤ 1.5× S1 instructions per mover; churn (125k lumberjacks: p99 ≤ 60 ms, ≤ 50 re-searches per tick); burst (10k orders on one tick: max tick ≤ 1.5 s, ≤ 256 MiB search memory, 0 deferred); class grid (gaps and decks at 0–90°, 5 radii); wall built across a march ≤ 2 route changes; net lines ≥ −4k | 22 |
| 5 | **Frames** | Frames per reachability class with merge, cross-section width, per-class gaps, taper with monotone slots, frozen per-rank `F`, local shrink, rigid/column modes, rotation and mirroring, re-form on new orders, profiles on events, progress arrival | `packing.rs`, 5-point probe, direct-order rays, beside-slot arrivals | B3, B4 (no negative slot velocity); 400/400 rams at pass, forest and river; mixed-army neck test (one frame, splits only when reachability differs, merges); AI re-issue storm (0 reversals, 0 reshuffles) | 10 |
| 6 | **Armies and attack-move** | Per-army phases, whole-formation portal admission, coherence corridors, arrival sync, reachable-only acquisition, perimeter spots, leash with hysteresis, parallel pursuit, acquisition and look caps removed | 8-army cap, refused retry, serial `step_attacker` | B7, B8; acquisition p99 ≤ 8 ticks from sight at 100k in contact | 8 |
| 7a | **Exact LOD in play** | Analytic movers and convoys in matches, frame observers | — | LOD oracle on every replay; R1 movement-only bench ≤ 120 core-ms | 8 |
| 7b | **Economy and combat sleep** (outside movement; R1 depends on it) | Gathering, building, training and combat on timer events; generational ids | Per-tick gatherer, builder and trainer loops | Sleep on/off hashes identical; **R1 closed-loop at 20 Hz on a quiet box**, 10 min | 18 |
| 7c | **Wire segments** (client and protocol) | Segments and frames on the wire; client evaluation; 128-seat cap lifted | Full per-unit frames | e2e; bytes per player per second; live R1 with 1,000 synthetic sessions | 8 |
| 8 | **Shards (later)** | Super-chunk partition | — | Two-shard replay identical to one | later |

**Total:** about 116 agent-days plus about 10 owner playtest cycles.

**Minimal proof of the 1M cost:** phase 0 plus S1, about 15 days and less than 15% of the spend.
Nothing behavioural ships before S1 passes.

**Owner-visible milestones:**
- **Tick rate (phase 1).** About day 25: the same behaviour, faster.
- **Avoidance (phase 2).** About day 35: villagers pass, there is no oscillation, enemies block, and
  R0 reaches 20 Hz.
- **The cutover (phase 4).** About day 65: no unit ever waits for a route.

**Riskiest phase:** the cutover. It is de-risked by:
- S1, which measures the kernel cost;
- avoidance landing first, so fields plug into a known step;
- regions as a separate earlier layer;
- the freeze on other movement PRs;
- merges at least every 3 days.

## 3. Gates and tests

1. **Determinism.**
   - Recorded-command replay for phase 1 and for thread counts.
   - The closed-loop in-process match for every behaviour phase. An open-loop Python log diverges as
     soon as behaviour changes.
   - Hashes identical at 1, 2, 6 and 12 threads.
2. **Oracles:**
   - LOD on against off;
   - the field LRU at 0, 1 and unbounded;
   - cold against warm caches;
   - sleep on against off.

   The snapshot oracle is dropped: today's snapshot is a resume, not exact state.
3. **Terrain suite on France tiles:**
   - a wall closes mid-column;
   - diplomacy flips at a gate;
   - a bridge is built and destroyed with units on the deck and ships below;
   - a foundation placed on a column;
   - a ram and infantry at a diagonal neck;
   - a ford with ships and land units;
   - a crowd at a ledge;
   - a forest with more than 255 pockets;
   - a parked army in a pass;
   - a rally pile of 2,000 units in a walled yard, which must sleep within 30 s.
4. **Deadlock suite:**
   - 2 × 100 villagers through a 1-tile gap;
   - armies head-on at a bridge;
   - a 4-way crossing;
   - a ring of mutual pushers;
   - ram against ram, transport against transport, and transport against a working fishing ship.
5. **Claims.** 64 claimants for 8 spots at 1 and 12 threads give identical winners.
6. **Performance.**
   - Instruction, cycle and DRAM-byte counters per stage.
   - Wall time only pinned to 6 physical cores, as the median of 3, with load and free RAM recorded.
   - Serial fraction from 1, 4, 8 and 12 threads.
   - RSS per rung envelope.
   - Forbidden-pass counters at 0.
7. **Live probe.** The march probe measures B1 from client send, plus an unreachable-goal probe. It
   is used for performance and for the owner's view, never as a ±10% gate.

## 4. Decisions taken (technical, owner may override)

- **Phase order:** avoidance before frames and before the cutover.
- **Gates:** keep today's rules: owner-only passage, and an open gate passes anyone. Allied passage
  is noted as an option.
- **Ids:** monotonic until phase 7b.
- **Snapshots:** stay resume-grade. The snapshot oracle is dropped.
- **Phase 7b:** economy and combat sleep is in the plan, because R1 depends on it, even though it is
  not movement.
- **R3:** 10M is resident and dilated. Aggregate economies for 20 Hz are a separate gameplay
  decision, not in this plan.

## 5. Round-3 amendments (they take precedence over the tables above)

### Phase 0

- **Gates.** Instruction counts only, through Callgrind. This machine refuses perf counters:
  `perf_event_paranoid` is 4 and no uncore PMU is exposed. Bytes come from Callgrind's cache
  simulation (LL misses × 64 B). If the owner later runs one root step (`perf_event_paranoid` ≤ 1,
  `amd_uncore`), real DRAM counters are added.
- **Runs:** 30 game-minutes at 100 seats, and 2,000 ticks at 1,000 seats. Waiting units are recorded
  and the starved baseline is labelled.
- **Determinism budget.** 3 days to fix divergences found at 1, 2, 6 and 12 threads; beyond that,
  record them and re-plan.
- **The semantic lint** starts with a ratcheting allowlist of today's caps, which shrinks phase by
  phase.
- **Reversals** are attributed by cause. Today's pin is 18 on the 1,000-tile march, not 26.

### S1 (12–15 days)

- **Inputs:**
  - census-built terrain, overlay, clearance and heights, counted in RSS;
  - `WalkChunk` reads at 0, with a counter;
  - the table mapped from a file, not a sealed memfd;
  - load logged next to every wall time.
- **Kernel contents:**
  - the presence index buckets;
  - pushee motion;
  - outward moves from over-cap cells;
  - exact vision counts with overflow.
- **Gates:**
  - movement work ≤ 135 core-ms at 60% calm and ≤ 190 at 30%;
  - serial ≤ 3 ms;
  - one extra run measures the calm share on a France R1 population with fights, chokepoints and
    commuters.

### Phase 1b: hot/cold unit split (new, 12 days, between phase 1 and phase 2)

`StoredUnit` (about 727 `.units` references) splits into S1's ≤ 64 B hot record, kept in slot
order, plus cold records.
- **Gates:** identical hashes; ≤ 64 B hot per mover; ≤ 300 B per unit (≤ 128 B by phase 4); RSS per
  unit.

### Phase 2 (15–18 days)

- **Serial gate** covers movement stages only. The planner, combat and the economy stay serial until
  their own phases.
- **Reversals:** contact-caused reversals at 0. The total of 0 moves to phase 5.
- **Shadow LOD oracle.** For every mover the predicate marks analytic, the closed form equals the
  full step on every tick until the predicted wake. The full on/off oracle arrives with 7a.

### Phase 4

Phase 4 removes the route, position, refinement, yield and server caps. The acquisition, look and
army caps go in phase 6, as listed there.

### Total

About **150 agent-days** plus about 10 owner playtest cycles.
