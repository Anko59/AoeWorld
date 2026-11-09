# Movement rework: AoE2 movement at the largest scale one machine allows

Design only, no code. **Version 4**, revised after three rounds of five adversarial reviews (round-3 amendments in §4.11)
([evidence §6](pathfinding-rework-evidence.md#6-review-log)). Code read at dev `19dc7fe47`
(#735, 2026-11-07).

Companions:
- [scale](pathfinding-rework-scale.md): the machine, every stage's cost, memory, exact level of
  detail, and how far this box goes;
- [delivery](pathfinding-rework-delivery.md): phases, gates, tests, risks;
- [evidence](pathfinding-rework-evidence.md): the current code, the owner's pain points,
  measurements, prior art, the review log.

*(est.)* marks numbers no run has confirmed.

**Scope.** "Movement" means everything a moving unit costs:
- routes, and applying them to units;
- steering, sidestepping and avoidance;
- collision and spatial indexes;
- formations, Generals and the attack approach;
- villager, animal and ship trips;
- the bookkeeping that moving causes: index relocation, vision, building memory, changed lists, unit
  and fog fan-out, hashing.

## 1. Verdict

The iterations fixed real symptoms: immediate starts when the fast attempt succeeds, large blocks
through a 2-tile gap, small same-type groups that recover. But production movement code went from
9.5k to 17.2k lines in 11 days, and the late game runs at 10–16 ticks/s. On dev, the #735 owner-like
load measures:

| Measure | Value |
|---|---|
| Tick p50 | 57–83 ms |
| Movement | 31–45 ms (apply + group apply 15–21, sidestep 4–6) |
| Route search | 7.6–9 ms |

So applying movement is the larger cost. Waiting for routes is the larger behaviour fault. Four
structural causes keep producing new bugs faster than patches remove them:

1. **Units wait for compute.**
   - Per-unit planners and tile routes run under world-fixed budgets.
   - About 15 other world-wide caps exist (§4.10). Under load, something is always waiting.
2. **Three geometries disagree.** The planner treats units as points, clearance is a mutexed field,
   and the swept circles carry deck and lane special cases.
3. **Shape comes from probes and timers**, with about ten overlapping "unit in the way" mechanisms.
4. **Bookkeeping scales with all units, all buildings or all sessions.** Every tick:
   - every unit is converted for the network;
   - changed lists are sorted;
   - every watched building is cloned for building memory;
   - every gate scans all buildings;
   - every felled tree scans all units;
   - the fog raster is rebuilt per human session.

**Recommendation.** Replace the movement core in phases that each ship alone. Behaviour stays AoE2,
protocol semantics are kept until the explicit wire phase, and a measured spike proves the
1M-unit cost before any behaviour phase starts.

The core changes from *each unit searches, waits, then follows waypoints* to this:
- every order gets a corridor on its tick;
- every mover always has a direction;
- formations follow the measured width of the passage;
- provably straight movers cost nothing until their next event.

## 2. Requirements and behaviour contract

**Binding requirements** (owner, 2026-11-07):
- the most units this machine can run, far beyond 10k;
- AoE2 behaviour;
- when overloaded the game slows, but never silently waits;
- level of detail only where positions equal full simulation;
- deterministic and integer, identical across thread counts;
- no legacy profiles.

Behaviour is measured on the closed-loop deterministic match and on the live probe
([delivery §3](pathfinding-rework-delivery.md#3-gates-and-tests)). Numeric thresholds are calibrated
against AoE2 DE reference clips recorded in phase 0, where clips exist. Until then they are this
plan's targets.

| # | Behaviour | Acceptance metric |
|---|---|---|
| B1 | Starts on the order tick (DE 153015) | Every member with free space takes its first step ≤ 1 tick after admission; p99 ≤ 3 ticks from client send at 100 players |
| B2 | Reachable goals are reached; unreachable means the closest reachable point | Per member, within 1.3× the ideal clearance route; 0 silent idles; snapped goals form the layout around the nearest frontier point |
| B3 | Formations at the slowest member's speed (Pottinger) | Slot error ≤ 1 tile in the open; class order never inverted; a new order re-forms the group |
| B4 | Squeeze through gaps and re-form after | 2 ram files and 4 infantry files through a 2-tile gap; no member's slot moves backwards; full width within 1 depth after the rear clears |
| B5 | Stuck parts recover without micro | No member still for more than 60 ticks unless its contact chain, or its slot chain (the member ahead in its file), moved within 60 ticks or is fighting; 0 groups ended by a timeout |
| B6 | Slide past friends, push idle ones, enemies block, no knots | 0 oscillations over 2 cycles; allied deadlock suite throughput > 0 within 10 s; enemies meeting at a bridge start combat with 0 escalations and 0 re-routes |
| B7 | A General's formations share chokepoints and arrive together | No interpenetration; layout within 2 tiles; arrival spread ≤ 10 s *after the narrowest crossing* (the head waits) |
| B8 | Attack-move engages what it sees, then continues | ≥ 90% of members with a reachable enemy in sight strike within 5 s; survivors reach the goal |
| B9 | Ships, transports and villagers follow the same rules | A transport passes a working fishing ship in a 1-tile river; every passenger boards or the journey fails visibly; the gather rate at a crowded mine is within 10% of uncrowded |

## 3. Design principles

- **P1. Cost follows orders, groups and events.** No per-tick pass touches all units, buildings,
  animals, journeys or sessions, except the full-fidelity kernel and its index. Every serial section
  is ≤ 2 ms and proportional to events. Counters prove it: each forbidden pass has a counter, gated
  at 0.
- **P2. Every mover always has a direction.** Corridors are goal-rooted (one reverse search settles
  every member's region). Gate: "movers without a direction" is 0 on every tick.
- **P3. One geometry.** One overlay holds:
  - passability and height edges;
  - buildings, roads and decks;
  - gated portals.

  The planner, the fields, formation width and the exact static sweep all read it.
- **P4. No deferred work.**
  - Every order is admitted on its tick. Superseded orders are coalesced within the tick, identical
    re-issues reuse their corridor, and anything past an abuse cap is rejected with a reason.
  - World-wide behaviour and capacity caps are removed. A **semantic lint** fails CI on any per-tick
    counter compared to a constant in behaviour code.
  - Overload dilates time.
  - Caches are pure functions of their full key. History-dependent data (L2 costs, corridors,
    penalties) is reference-counted state that is hashed.
- **P5. Locality and previous-tick reads.** Every interaction between units reads double-buffered
  previous-tick state. Every contested claim (spots, reservations, leases, cargo, garrison and gather
  targets) merges canonically by (tick, priority, id). Results are therefore independent of thread
  count, and a later shard boundary is the same read.
- **P6. Delete as you replace.** Mechanism tests go with their mechanisms. Scenario tests that
  reproduce the owner's bugs stay. Every PR deletes something.
- **P7. Exact level of detail.**
  - Units whose next ticks are provably a straight segment under the full rules are stored as that
    segment and woken by events.
  - Every reader evaluates the closed form. The hash covers observable state only.
  - Gate: LOD on and LOD off give identical hashes every tick. The rules of phases 2–6 are written to
    the analytic predicate from the start.

## 4. Target architecture

```text
order ─► admit on tick ─► reachability per (component, class, gate permission) ─► goal-rooted corridor
                                                                                   │
frames (split only when reachable sets differ) ◄── centerline + cross-section profile ◄┘
  │ taper ahead of narrowings; slots never move backwards
  ▼
goal velocity = slot (hysteresis) | field lookahead | direct pursuit to perimeter spot
  ▼
hardness-based separation on previous-tick state ─► exact static sweep ─► step or analytic segment
  ▼
events (tile/chunk crossing, time-to-contact, segment end, player event) ─► partitioned timer wheels
```

This is AoE4's published data flow (GDC 2022): portal graph, flow only along the route, a virtual
leader with slot steering, and "extended flow" for mixed sizes. The additions here are for scale,
exactness and the failure cases the reviews found.

### 4.1 Geometry

- **Overlay page per chunk.** Built in the tick's pre-pass, never inside the kernel. It is a pure
  cache keyed by the passability versions of the chunk and its 3×3 neighbourhood. Bits per tile:
  - passable;
  - 4 height-edge bits (cliffs: |Δh| > 1 is a wall);
  - building;
  - road;
  - deck;
  - gate.

  Gate ids go in a sparse side list. Gather strokes never bump a version. Ordered maps leave the
  per-mover path.
- **Clearance.** `u8` per tile in quarter-tile units, plus a baked per-chunk minimum summary.
  Sub-tile widths are computed from overlay bits at query time, rounded down by the sampling error.
- **Bridge decks** are analytic capsules (today's `collision/bridge.rs` test), not rasters. A ram has
  a 51-subunit margin on a 1-tile deck, and quarter-tile rasters erode up to ~180 at oblique angles.
  - Building a bridge splits water; removing one splits land. Both are split events.
- **Gates** are owner-labelled gated portals.
  - Reachability per player admits only the gated edges that player may pass.
  - The physical open state stays where it is today: the hard constraint. An open gate passes anyone,
    which keeps the "slip in while open" play.
  - Field keys carry the open bits of the gates in their 3×3 chunks only.
  - Presence events on each gate's ring replace the `advance_gates` scan of all buildings.
  - Allied passage is an owner decision. The default keeps today's owner-only rule.
- **Heights** stay resident in the table, because combat reads them on every hit. No tick path reads
  the generator's `WalkChunk`.
- **Static collision** is an exact swept circle against this geometry. Sliding uses the contact
  normal.

### 4.2 Regions and reachability

- **Labels.** `u16` region labels per (chunk, class bucket). There are 5 land radii (205, 256, 308,
  410 and 461) in 3 buckets: small ≤ 256, medium ≤ 410, large ≤ 461. Every bucket fits a 1-tile gap,
  as today (`contact.rs:16-18`). Edges carry a 2-bit minimum-bucket tag.
- **Union-find.** Two levels, chunks then super-chunks, with canonical minimum-key representatives.
  Splits are batched once per tick.
- **Reachability** is per (component, bucket, gate permission). The nearest reachable point is
  projected around the frontier point closest to the goal, and the layout is placed there. This
  deletes the pocket proof, the certified flood, the unreachable memo and the sealed-ground deadlines.

### 4.3 Graph and corridors

- **L1.** One bucket-tagged graph.
  - Open chunks are implicit (SupCom2's clear sectors). Elsewhere costs are `u16` deltas over the
    octile bound.
  - Trees are a bitmap. Records remain only for gold, stone, forage and fish.
  - An implicit directory gives chunk offsets.
  - **Measured on France in phase 0** (an encoder census) before the map size and the R3 line are
    fixed. Target ≤ 700 B per chunk.
- **Dynamic state.** Buildings and felled trees become per-chunk bitmaps; partial amounts are
  `(u16 tile, u16 amount)`.
  - This deletes the 2²⁰ depletion cap, beyond which gatherers silently earn nothing, and the 2²⁴
    obstacle cap.
  - Dynamic L1 for changed chunks is a keyed cache with its own budget line.
- **L2.** Super-chunk border runs with sparse `u16` edges, and ALT landmarks as `u16` rounded down.
  Routes are within 1.3× optimal.
- **Corridors.**
  - Goal-rooted reverse search that settles every member's region.
  - Reference-counted state that lives exactly as long as the orders using it, hashed. There is no
    LRU over state.
- **Re-search rules.**
  - Openings (felled trees) never re-search; they may only improve a snapped goal.
  - A closure re-searches only if it cuts a corridor portal run below the bucket's width.
  - A route switches only if it is at least 10% cheaper.
  - Re-searches are coalesced per corridor per tick.
- **Blocker escalation.**
  - It fires only for hard *non-combat* blockers: never an enemy in a fight or in sight on
    attack-move, never a pushable friend.
  - It needs demand with zero throughput for N ticks, where N > the arbitration phase + lease.
  - The penalty is per (alliance, edge), decays, and is hashed.
- **Water** uses the same graph in its domain. The embark graph joins coasts and handles fords, which
  sit in both domains. Cross-domain hardness between ships and land units on a ford is explicit (§4.6).

### 4.4 Fields and steering

- **Windows.** A field targets the frame's cross-section width at the portal crossing. Non-formation
  units target the whole run, which avoids an hourglass bead every 32 tiles.
- **Storage.** Open chunks are analytic (no storage). Elsewhere 3-bit directions plus a
  line-of-sight flag, about 0.5 KiB.
- **Key** covers every input: 3×3 versions, the bucket, the local gate bits and the window.
- **Build.** A pre-pass collects the keys and builds missing fields in parallel. Fields are read-only
  during movement.
- **Oracle.** The LRU at size 0, 1 and unbounded gives identical hashes.
- **Steering** aims at an 8-tile string-pulled lookahead.
  - The slot/field mode starts in slot mode if the slot ray is clear on the admission tick, and
    switches back to slot only after 10 clear ticks.
  - Villagers try a clearance line of sight first.
  - Pursuit goes straight to the target's perimeter spot while its ray is clear.

### 4.5 Formations

- **Frames per reachability class.** A group splits into frames only when members' reachable sets
  differ; a ram that cannot pass a neck the infantry can is one example.
  - The corridor class is the largest bucket in the frame.
  - Frames merge again when their reachable sets re-join.
  - A mixed army that can all pass stays one frame.
- **Width.** `w(s)` is the wall-to-wall cross-section along the normal, from quarter-tile samples,
  rounded down.
  - Occupancy is folded in only for occupants with h ≥ 0.75 (workers, enemies), never pushable idle
    friends.
  - Decks are read analytically.
- **Files.** `F = floor((w − d_max)/gap) + 1`.
  - The infantry gap is about 450 subunits (near DE's 0.40-tile champion spacing; to be calibrated).
  - Other bands use diameter plus the same margin.
  - Today one 1,040 gap serves every class, so lines are too sparse.
- **Taper.** The file count shrinks over `(F0 − F)/2` rank depths *ahead* of a narrowing, at about
  45°.
  - A member's slot arc position never decreases; it holds instead.
  - Renumbering happens only inside the taper.
  - Each rank's `F` is frozen from entering a narrowing to leaving it.
  - A slot unreachable for 10 ticks shrinks `F` locally.
  - The head slows at the exit by a geometric throughput ratio, so the block re-forms near the gap.
- **Curves.**
  - Rigid mode applies only while the whole footprint has clearance; otherwise the group switches to
    columns, with hysteresis.
  - For an order turn above 45°, slots rotate about the anchor, with 1.5× catch-up. Wheeling against
    rotation is decided from DE clips.
  - A turn above 120° mirrors the rank map, and class order is restored by rank swaps.
- **Membership.**
  - Deaths compact the ranks.
  - A new order to a group in formation re-forms it, with slots assigned by minimum crossing. Only a
    same-goal re-issue keeps offsets.
- **Profiles** are recomputed on segment events (a centerline extension, a narrowing boundary), not
  every tick.
- **Arrival** is progress-based, per member.

### 4.6 Avoidance

- **Hardness.** `h` is the fraction of `r_a + r_b` that may never be entered. Below it the step is
  clamped; between `h·(r_a + r_b)` and `r_a + r_b`, speed drops linearly with overlap.

  | Pair | `h` |
  |---|---|
  | Same frame | 0.5, except during rank swaps |
  | Same player, moving | 0.5 |
  | Idle friend | 0.5 |
  | Ally | 1.0 |
  | Working unit | 0.75 |
  | Enemy | 0.95 (lines and bridge blocks hold) |
  | Hulls in lanes narrower than two hulls | 0.45 |
  | Ship against land unit on a ford | 1.0 (no ghosting) |

- **Half-gap rule.** Each unit consumes half of each pair's gap from previous-tick state, with all
  neighbours ordered by (distance, id). At most two bodies share a point. A unit whose cell is over
  the cap does not move that tick.
- **Wake on contact.** Contact with a sleeping or working unit wakes it into the kernel that tick, as
  a deterministic event. Pushing and yielding work, and a pushed worker returns to its home spot.
- **Passing.** Side memory has two fixed 8-byte slots per unit (Pottinger). The heading comes from
  the lookahead. Units fall back to the left when clearance on the right is below their radius. On
  crossing paths, the unit with the other on its right slows.
- **Narrow portals, within an alliance only.** Enemies are never scheduled: contact is combat.
  - Whole frames take contiguous leases. Villagers pass between frames.
  - Direction alternates by demand: when one side's queue empties, or after K units.
  - Order within a direction: order tick, then id. Size priority resolves only a head-on pair inside
    a lease.
  - Leases lapse when there is no progress.
  - Every pace controller is a geometric function, and a frame takes their minimum: curve,
    throughput, lease and arrival sync.
- **Kept and restored:**
  - destination reservations (arrival contagion);
  - resting relaxation, which stops at a packing density, after which units sleep;
  - ejection from new foundations;
  - the work-spot density cost.

### 4.7 Armies and Generals

- Each army decides on its own phase (`id % 10`), in parallel. This deletes the 8-army world cap and
  the 256-tick refused retry.
- Corridors carry congestion and coherence costs, so the army does not split across a distant ford.
- A narrow portal admits whole formations in turn. The next waits until the previous rear has fully
  cleared, never sharing width.
- Formations pace so the army arrives together in its battle layout.
- The army survey reads per-chunk aggregates.

### 4.8 Attack-move and combat movement

- Members acquire only targets reachable, or in range, from their own (component, bucket). An enemy
  across a river is not a melee target.
- Melee goals are today's perimeter spots (`combat/spots.rs`); ranged units stop at range. Rear
  members with an enemy in sight wrap the flanks.
- The leash of one formation depth applies only to new acquisitions. Re-acquiring after a leash
  return takes a cooldown.
- Rank swaps finish before the frame is in sight of the enemy.
- Pursuit runs in the parallel kernel. Today `step_attacker` is serial over every attacker.

### 4.9 Bookkeeping caused by movement

- **All-unit presence index.**
  - Morton-sorted, 8 B per unit plus `u32` chunk offsets, with per-chunk and per-4-tile-cell counters.
  - Updated at chunk crossings and sleep/wake transitions.
  - The mover grid is a sub-view: an incremental sorted 8-byte (cell, slot) index. The SoA is
    permuted only at epochs.
- **Time-to-contact wakes.** Analytic movers wake on the time-to-contact bound against the occupancy
  of the 4-tile cells their swept bound crosses, not on 3×3-chunk presence.
- **Player-wide events** re-segment that player's movers: speed research, age-up line of sight.
- **Vision** counts each player's sight in saturating `u8` counters per active chunk.
  - Each tile crossing updates only the edges of the sight circle (about 0.15 µs), with no
    allocation.
  - A convoy's frame observer updates by the same edges.
  - Explored tiles are stored as runs.
- **Building memory.** Remembered buildings shrink to 24 B records, updated only on a building change
  or a watch gain. Today every watched building is cloned every tick.
- **Fan-out.**
  - A dirty list. Fog is sent as deltas per chunk revision, built in parallel.
  - Session diffs are dirty-driven, never under the lock in O(view). Own units are dirty-only.
  - Session queues are byte-bounded.
  - AI seats read through an in-process query API, with no sessions.
- **Other passes made event-driven:** gates, wildlife, transport journeys and `apply_watch`. Each
  carries a counter gated at 0.
- **Animals** move through the same kernel and collision as units. Today they are a second movement
  system with no unit collision and a vision update every tick.
- **Timer wheels** are partitioned by super-chunk, ordered by (tick, kind, id), and merged
  canonically.
- **Hash** is on demand at checkpoints and covers observable state only. Index slots leave it.

### 4.10 Caps removed (complete inventory, phase 0 verifies)

| Cap | Today |
|---|---|
| Route search | 16,384 fair work per tick; 2 near + 2 far slots per player; 400 searches world-wide |
| Position searches | 4 per tick |
| Attack searches | 16 per tick |
| Gather repicks | 4 per tick |
| Navigation preparations | 8 per tick |
| Refinements | 4 per tick |
| Detaches | 4 per tick |
| Yields | 8 per tick |
| Idle visits | 32 per tick |
| Idle acquisitions | 128 per tick, plus 16,384 target inspections per tick shared with towers |
| Formation looks | 16 engagements and 128 members |
| Scouting | 8 jobs and 16 searches |
| Generals | 8 armies per 10 ticks |
| Server commands | 32 per tick match-wide; 64 pending per connection, 1,024 global |
| Capacity: `MAX_ECONOMY_PLAYERS` | 128 (also caps animals; blocks R1 inside the simulation) |
| Capacity: depletion records | 2²⁰ |
| Capacity: obstacle tiles | 2²⁴ |

### 4.11 Round-3 amendments (they take precedence over the sections above)

**Pushing and crowds**

- **Pushing is a mechanism.** A pushee's goal velocity is the pusher's intended overlap, projected on
  the contact normal and clamped by the pushee's own half-gaps. It moves one hop per tick.
- **Over-cap cells** admit only moves that lower their count. They never freeze.
- **Coincident pairs** separate along a direction derived from hash(id pair).
- **The exact sweep** allows any step that does not deepen an existing overlap. This is the
  successor to `static_overlap_escape`.
- **Gate (phases 2 and 4):** 5 passengers unloaded on one point, 20 units ungarrisoned, and 30 units
  under a new foundation. All move within 2 ticks and are separated within 40.
- **Sleepers woken by contact** join the kernel at T+1. The sleep predicate keeps a one-tick margin,
  so push chains stay order-independent.
- **Head-on pairs choose their sides once.** The lower id picks from joint lateral clearance, and
  both commit to opposite sides until they have passed.
- **When several `h` rows apply,** the most restrictive wins. The one exception is hull against hull
  in a lane narrower than two hulls, which uses 0.45.

**Formations and portals**

- **Formation types** are per-rank lateral occupancy masks:

  | Type | Rule under narrowing |
  |---|---|
  | Line | Filled ranks |
  | Flank | The wings close first, centre hole included |
  | Box | Keeps its inner class in the centre files |
  | Staggered | Drops its offset while `F < F0` (it keeps today's 1,560 gap) |

  B3 and B4 are gated on all four types.
- **Leases at narrow portals.**
  - A lease covers the portal plus an apron as long as the next frame's taper.
  - Inside an army, a frame re-forms while it advances at army pace.
  - The next frame is admitted once the previous rear is one taper length past the exit.
  - The demand threshold `K` never splits a frame's lease.
- **Blocker escalation** counts per portal: 40 ticks of demand with zero throughput while no lease is
  active there.

**Corridors, decks and gates**

- **Closures** also re-search when they split a region on the corridor's region chain, not only when
  they cut a portal run.
- **Gate:** a building placed between a corridor's portals gives 0 movers without a direction and at
  most 1 re-search.
- **Decks in the region graph.** Each bucket gets explicit deck edges from the capsule test. Raster
  cells that touch only at corners never decide deck reachability.
- **Gate:** the class grid is run against reachability in phase 3.
- **Gates are keyed on permission, not on open state.**
  - For the owner a gate is passable in fields and corridors; for everyone else it is a wall.
  - The open state stays only in the hard constraint, so owners can approach and open their gates.
  - Ring events also cover any unit inside the gate rectangle, and deaths.
- **Gate:** an army leaves and re-enters its walled base through a closed gate, and 1,000 toggles
  cause 0 field rebuilds.

**Vision, building memory and indexes**

- **Vision counts are exact.** A `u8` per tile escapes past 255 into a per-chunk sparse `u32`
  overflow table.
  - A convoy observer applies the exact union of its members' circle edges.
  - The LOD oracle hashes vision.
  - Test: 300 observers leave one tile in any order, and the tile fogs on the exact tick.
- **Building memory.**
  - Watched buildings are read live.
  - A remembered ghost record (about 48 B, covering every wire field) is written only when a watch
    is lost. Research, queues and bridge span go in a cold optional record.
  - The record change is part of phase 1's named repin. Dropping enemy queues from the wire is a
    phase-7c owner decision.
- **The presence index** uses per-chunk buckets with slack, compacted at epochs. It never shifts a
  sorted CSR array.
- **The phase-1 cell index** is an open-addressed table over occupied tiles. It keeps today's
  per-cell swap-remove order, so Gauss-Seidel results are unchanged. It is never a dense per-tile
  array.

**Hashes**

There are two, each with a stated use:
- a full state hash (corridors, penalties, leases, side memory), every tick in tests, for thread
  counts;
- an observable hash (positions as evaluated, vision, explored tiles, remembered state) for the LOD
  and sleep oracles.
