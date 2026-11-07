# Movement rework: scale on this machine

Companion to [the rework plan](pathfinding-rework.md), version 4. *(est.)* marks figures no run has
confirmed. Phase 0 and the S1 spike ([delivery](pathfinding-rework-delivery.md)) replace them with
measurements.

## 1. The machine and its verdict

- **Hardware:** an AMD Ryzen 5 4600H, 6 cores / 12 threads at ≤ 3 GHz, 2 × 4 MiB L3.
- **RAM:** 14 GB, with 4–5 GB free while other agents run. Swap was full when reviewers measured it.

**CPU per 20 Hz tick:** 6 × 1.25 SMT × 50 ms ≈ 375 thread-ms. That leaves about **300 core-ms on a
quiet box** and **about 225 shared**, for all systems.

**Bandwidth:** about 20 GB/s, so about 1 GB per tick (*(est.)*; phase 0 runs a STREAM baseline). With
movement at 40% of it, about 950k full movers are the bandwidth ceiling.

**Memory envelopes:**

| Rung | Envelope |
|---|---|
| R1 and R2 | ≤ 5 GiB RSS on the shared box |
| R3 | ≤ 8 GiB on a declared quiet box |
| The S1 spike | ≤ 1.5 GiB, excluding the mapped table |

| Rung | Units | Verdict |
|---|---|---|
| R0 | ~10–15k, 100 players | Today 10–16 ticks/s late game. **20 Hz is gated at the avoidance phase** |
| R1 | **1M**, 1,000 players, France 16k² | **Committed target, conditional on S1.** About 17–20 Hz on a quiet box: serial sections total ≤ 5 ms, and parallel efficiency is about 85%. Expect 15–18 Hz shared. 20 Hz needs ≥ 70% calm. Conditions: a full mover ≤ 2,000 instructions (≈ 650 ns); ≥ 60% calm at R1 density; economy and combat sleep (phase 7b, not movement); building memory and fog event-driven; in-process Rust bots |
| R2 | 3–5M | **Stretch.** About 15–20 Hz calm, 9–12 Hz in contact |
| R3 | 10M, 10k players, ≤ 32k² | **Resident on a quiet box** (≈ 7 GB): about 17 Hz while mostly asleep, **2–5 Hz in mass contact**. Never 20 Hz with every unit individual. 20 Hz would need aggregate economies, a gameplay change left to the owner |

### R1 sensitivity, in core-ms per tick

| Case | Core-ms | Rate |
|---|---|---|
| Plan values (650 ns, 70% calm) | ≈ 300 | ≈ 20 Hz |
| Calm share 30% | ≈ 380 | ≈ 16 Hz |
| Kernel 1.3 µs | ≈ 360 | ≈ 17 Hz |
| Both | ≈ 500 | ≈ 12 Hz |
| Building memory left as today | +50 serial | caps at ≈ 14 Hz |

## 2. Stage budget, R1

The R1 workload: 1M units; 300k moving (90k full fidelity and 210k analytic or convoy); 100k in
contact; 600k asleep.

| Stage | Today | Target per unit | Core-ms | MB per tick |
|---|---|---|---|---|
| Admission, corridors, L2 searches (~30 groups + trips per tick) | n/a | 0.5 ms per search | 15 | 1 |
| Frames: advance plus profiles on segment events (3k) | n/a | 5–10 µs | 20 | 2 |
| Full-mover kernel | ~3.6 µs | ≤ 650 ns | 59 | 38 (420 B) |
| Analytic events: time-to-contact wakes, tile crossings | n/a | 210k ÷ 10 ticks × 0.5 µs *(wake rate measured in S1)* | 10 | 3 |
| Incremental mover index | 4 hash-map relocations | 20 B, ~1/18 change cell | 2 | 2 |
| Presence index at chunk crossings | n/a | ~0.1 µs per crossing | 1 | 1 |
| Vision edge updates | ~2.5 µs, allocating | 0.15 µs per crossing | 3 | 2 |
| Building memory (event-driven) | ~0.5 µs per building per tick, serial | per change | 1 | 0 |
| Dirty list and fan-out (units + fog deltas) | every unit + per-session rasters | 100 ns per dirty subscribed unit | 30 | 30 |
| Fields: pre-pass and builds | n/a | analytic when open; ~20 µs per build | 3 | 1 |
| Overlay churn: depletions, L1, regions | all units per change | ~0.5 ms per changed chunk | 10 | 1 |
| Generals (1,000 armies, phased) | 8 per 10 ticks | ~100 µs per decision | 10 | 1 |
| Animals in the kernel | separate system | counted with movers | 5 | 2 |
| In-process AI | Python sessions | ~20 µs per seat | 20 | 5 |
| Combat contact (not movement) | serial per attacker | 1 µs, parallel | 100 | 40 |
| Economy events (not movement, phase 7b) | serial per gatherer | 0.3 µs per event | 15 | 5 |
| **Total** | | | **≈ 300** | **≈ 135** |

**Serial sections**, gated at **≤ 5 ms per tick as a sum** and proportional to events:
- the corridor commit;
- the event and claim merge;
- the union-find;
- the index prefix sums;
- the timer-wheel merge;
- command application;
- session delivery.

## 3. Memory

**A field-level layout is committed before S1.**

**Hot mover record:** ≤ 64 B, about 52 B fixed today *(est.)*:

| Field | Bytes |
|---|---|
| Position | 8 |
| Previous position | 8 |
| Segment `(t0, v, carry, t_end)` | 20 |
| Goal ref (frame + slot, or field key) | 8 |
| Class, flags | 2 |
| Side memory (2 × 8) | 16, in a cold side table |
| Radix and wheel references | 8 |

**Unit all-in:** ≤ 128 B.

| Item | Today | Target | R1 (1M, 16k²) | R3 (10M, 32k²) |
|---|---|---|---|---|
| Units | 0.8–1.2 KB | ≤ 128 B | 128 MB | 1.3 GB |
| Formation members | ~130 B | ≤ 8 B | 3 MB | 30 MB |
| Id vectors | ~50 B per id ever issued | Monotonic ids until phase 7; then generational | 16 MB | 160 MB |
| Presence index and counters | none | 8 B per unit + offsets | 9 MB | 84 MB |
| Corridors and timer wheels | none | ~300 B per active group; 8 B per mover | 12 MB | 110 MB |
| Static map (L1, resources, heights, directory) | ≤ 7.1 KB per chunk | ≤ 700 B *(census)*; open chunks implicit | 175 MB | 730 MB |
| L2 + ALT | none | Sparse `u16` | 10 MB | 60 MB |
| Dynamic overlay and L1 cache | BTree, capped | Bitmaps + keyed cache | 60 MB | 450 MB |
| Clearance and overlay pages | Mutexed pages | Pre-pass pure cache | 64 MB | 128 MB |
| Fields | none | Analytic or 0.5 KiB | 64 MB | 256 MB |
| Vision | 4 KiB `u32` per chunk per player | `u8` counts with exact `u32` overflow per active chunk; explored as runs | 75 MB | 950 MB |
| Remembered buildings | ~250 B clones per tick | Live reads; ~48 B ghost on watch loss | 100 MB | 960 MB |
| Fan-out and sessions (humans only) | 64 × 16 MiB queues | Byte-bounded | 30 MB | 100 MB |
| AI (in-process) | ~2 MB per bot | ≤ 64 KiB per seat | 64 MB | 640 MB |
| Other gameplay, plus 15% slack | | | 500 MB | 1.6 GB |
| **Total** | | | **≈ 1.4 GB** | **≈ 7.5 GB** |

**Static table residency.** The static table must be evictable. Map it directly or through ext4
verity (phase 0 checks support on this filesystem), not through a sealed memfd, which pins it in RSS
while swap is full. The 1M-chunk and 2 GiB caps are lifted through the implicit directory.

**Largest map for 10k players:** 32k² committed, about 57k² at the ceiling. The France census in
phase 0 can revise this.

## 4. Exact level of detail

- **Analytic movers.** A mover is stored as a straight segment when the full rules provably produce
  one:
  - no body within its swept bound, judged by 4-tile-cell occupancy and the time to contact;
  - no speed change at an upcoming tile (road, mud);
  - no direction change before the segment ends.

  Positions are the closed form with the step's own integer carry, so skipped ticks land
  bit-exactly.
- **Wakes.** A timer wheel wakes an analytic mover for:
  - a tile crossing (vision);
  - a chunk crossing (the index);
  - the segment end;
  - a time-to-contact expiry;
  - an occupancy change in a crossed cell;
  - a player-wide event: research or an age-up line-of-sight change.
- **Readers.** Every position reader evaluates the closed form. Readers include gate rings, wildlife
  sensing, towers, acquisition, the survey and footprint validation.
- **Convoys.** Where the rules already lock members to slots at error 0, an army's slot-locked frames
  form one convoy. A frame observer covers vision.
- **Sleep.** Idle and working units sleep. Contact wakes them as a deterministic event.
- **Oracle.** LOD on and off give identical observable hashes on every tick at 1, 4 and 12 threads.
  Cases: research mid-march, an age-up, a gate ring, a wolf sensing an analytic villager, a road
  entry. It lands with the avoidance phase and runs on every replay after.
- **Calm share** is measured at R0 density in phase 0 and at R1 density in S1. Today's matches are
  about 100× sparser than R1.

## 5. Parallel tick

Phases run in this order. Each is parallel with canonical merges:

1. **Admit** orders: coalesce, reject past the abuse cap with a reason.
2. **Search corridors** over a read-only snapshot.
3. **Pre-pass:** overlay pages, clearance and fields.
4. **Advance frames.**
5. **Kernel** on previous-tick buffers. This step includes pursuit and animals.
6. **Analytic events.**
7. **Apply** (each unit writes itself), then the index updates.
8. **Merge** events and claims by (tick, priority, id).
9. **Overlay, regions, union-find.**
10. **Fan-out** from a double buffer, pipelined with the next tick.

## 6. Sharding (later, for live multiplayer)

Not relied on, but kept possible:
- super-chunk partitions;
- previous-tick reads;
- a replicated L2;
- chunk-owned fields, regions and L1;
- small replicated frames.

## 7. Other performance findings

These are scheduled in [delivery](pathfinding-rework-delivery.md):
- the server `UnitFrame` builds every unit, and its `IdTable` falls back to a HashMap past 2²⁴ ids;
- `Tables::refresh` skips players ≥ 4,096;
- the 128-seat protocol cap;
- the tick counter increments mid-pipeline (`game_movement.rs:356`). Moving it shifts every later
  salt, so it goes with the grid repin;
- the wildlife AI makes 9 `HashMap` probes per animal per tick.
