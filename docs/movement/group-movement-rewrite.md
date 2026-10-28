# Group movement and avoidance rewrite

Design only; code read at `a6a6b34f` (`ship/pathfinding`). Builds on
[aoe2-model.md](aoe2-model.md) and [validation.md](validation.md); answers the
owner's playtest: large-group movement makes the game unplayable.

Tags: **[F]** established fact with source; **[G]** guess, folklore or an
AoeWorld choice. Code citations are `file:line` relative to
`crates/simulation/src/game_world/` unless a path says otherwise.

## 1. How AoE2 moves groups

[A1] Pottinger, "Coordinated Unit Movement" (1999)
<https://www.gamedeveloper.com/programming/coordinated-unit-movement>;
[A2] "Implementing Coordinated Movement"
<https://www.gamedeveloper.com/programming/implementing-coordinated-movement>.
They describe Ensemble's techniques, not a spec of the shipped game **[G]**.

- **Commander [F A2].** Only the group's commander pathfinds; followers reuse
  its path (one search per group).
- **Speed matching [F A2].** Group speed = slowest unit. Cohesion levels: same
  speed; plus same path; plus same arrival (leaders wait, laggards get a
  temporary boost). An incohesive group is "only a UI convenience".
- **Slots [F A2].** Facing-relative offsets filled inside-out by a distance /
  collision / time heuristic; a placed unit outranks later arrivals.
- **Forming [F A2].** Formed before moving; a line *wheels* at turns. DE later
  dropped in-place assembly (below).
- **Obstacles [F A2].** *Scaling* (shrink offsets), *ordered avoidance* (break
  and re-form past it), *halving* (two sub-groups rejoin). Never single file.
- **Shapes [F HG]** (<https://aok.heavengames.com/university/game-info/general-info/formations/>):
  Line: melee front, ranged behind, siege/monks back; Staggered: same, wider;
  Box: melee outside; Flank: Line in two halves. Box spacing follows the
  largest body [F, forum only:
  <https://forums.ageofempires.com/t/poll-revise-box-formation-to-be-more-useful/241486>].
  Villagers, trade carts and fishing ships never join formations [F, openage
  reverse engineering:
  <https://github.com/SFTtech/openage/blob/master/doc/reverse_engineering/game_mechanics/formations.md>].
- **Stances and attack-move [F community; distances G].** Aggressive chases
  anything seen; Defensive chases briefly; Stand Ground attacks in range; No
  Attack never starts
  (<https://forums.ageofempires.com/t/defensive-stance-is-useless/169389>).
  Attack Move is a DE hotkey; 50292 added it for the AI. A plain move ignores
  enemies until arrival [G, widely held].

DE notes (`ageofempires.com/news`, 107882 via devtrackers.gg) [F]:

| Update | Group-movement change |
| --- | --- |
| 44725 | Moves under **20 tiles** take the most direct path. |
| 99311 | A single unit joins a moving group; no cycling to the back per click; a fast unit leaving a slow group is no longer kept slow. |
| 107882 | Line becomes a *column* only on orders over **30 tiles** (was 10); standing units no longer re-form when one leaves. |
| 141935 | Fewer formations breaking around small obstacles. |
| 153015 | "Escape" move goes straight without assembling; front units no longer regroup before advancing; formations attack instead of breaking. Big groups at chokepoints remain known. |
| 185872 | Better slot assignment (nobody first walks away from the goal); ranged formations don't break on out-of-range targets; units no longer stop when an enemy blocks their path. |

So: in-place assembly is a defect; the column is a long-move mode of
undocumented shape [multi-file G]; joining and leaving is per unit.

## 2. Verified causes of the owner's symptoms

Found by reading code; PR 0 (§6) turns each into a failing scenario.

**S1. A long single-file line, slow to form.** Every multi-unit military
right-click is a formation order (`crates/client/src/village/orders.rs:177-188`);
the AI does the same (`ai/aoe_ai/play/combat.py:308`).
`update_formation_column` (`formation/column.rs:50-99`) switches on `column`
when any member's straight tile ray to its slot is blocked, longer than 96
tiles (`formation/steering.rs:237`), or out of budget (`formation_line_clear`
returns false at `work == 0`, `steering.rs:241-244`), and then sticks
(`column.rs:89`). The budget is 16,384 probes per tick for all groups
(`steering.rs:5`), 4,096 per group (`column.rs:64`). In column mode each
member targets its predecessor minus 1,040 subunits, about one tile
(`column.rs:101-121`), whatever shape was chosen. The anchor starts at the
centroid (`formation/orders.rs:97`) at half speed until cohesive
(`formation/anchor.rs:23-27`, column gaps ≤ 6 tiles at `:137-147`), and
members only get the slowest speed (`steering.rs:132-134`).

**S2. One bump slows the whole line.** Column targets chain off the
predecessor's *actual* position (`column.rs:118`); a stretched gap halves the
anchor (`anchor.rs:23-27`). Outside formations, a mover with a same-heading
unit just ahead gets speed 0 and retries every 8 ticks (`spacing.rs:40-48`,
`game_movement.rs:97-104`, `movement_step/mod.rs:220-224`).

**S3. Everyone stops on first arrival.** Anchor arrival clears the leader's
order (`anchor.rs:75-80`), so `navigating` is false (`steering.rs:34`). Member
goals come from budget-limited rays (`steering.rs:63-125`) whose cache key
holds the anchor tile (`:54-62`), so caches churn, the budget runs out and
members fall to `Goal::Hold` (`:93`, `:130`). With `navigating == false` a
`Hold` member is "at goal" and becomes `Idle` (`steering.rs:157-167`): head at
the destination, tail standing still. Members cannot fight meanwhile
(`combat/acquisition.rs:84-86`).

**S4. Re-ordering some units stops the others.** `break_formation` deletes the
whole group and stops every member (`formation/mod.rs:72-91`). It is called
per re-ordered unit by `issue_formation_move` (`formation/orders.rs:54-56`),
`issue_task_move` (`route_scheduler/order.rs:61`, `:88`) and `stop_movement`
(`combat/lifecycle.rs:55`), so one death halts the army via `kill_unit`
(`lifecycle.rs:69`). A test requires it (`tests/formations/pressure.rs:143`).

**S5. Attack-move: no indicator, never resumes.** The client sets
`data-order-mode="attack-move"` (`crates/client/src/village/building_panel.rs:216-221`)
but `web/interface/game.css:11` styles only `move` and `build`.
`engage_formations` (`formation/combat.rs:33-39`) deletes the group (and its
destination and queue) and sends every member at one target; `end_attack`
then leaves them `Idle` (`combat/tick.rs:295-311`) with no march to resume.

**S6. AI units don't fight back.** The AI marches without `attack_move`
(`ai/aoe_ai/play/combat.py:308-309`); acquisition skips moving, planning and
unsettled-formation units (`acquisition.rs:77-86`); retaliation skips moving
units (`combat/retaliation.rs:14-17`). A settled member that does retaliate
breaks and stops the whole group (`acquisition.rs:124-131`).

**S7. Tile exclusivity too strict.** See §4.2.

## 3. Group movement design

Principles: one route search and one slot layout per order; O(1) unit work
per tick with no rays or searches; soft bodies between units (§4); no unit
waits for the group except through speed matching. Changed in place, no V2
profile (owner rule: no legacy before v1.0); replays and pins regenerated.

### 3.1 Data model

```text
Group  { id, player, form, members, route (one planner, existing scheduler),
         anchor_progress, destination, facing, pace, queue, attack_move }
Member { id, slot (offset at destination),
         state: Marching | Engaged | Arrived | Detached, resume: Option<GroupId> }
Unit.group: Option<GroupId>   // replaces Unit.formation (a leader unit id)
```

A group id ends the "first member carries the scheduler" coupling
(`formation/mod.rs:1-2`).

### 3.2 Slots at the destination

Slots are laid out around the *destination*, facing from the centroid, in
AoE2 class order (Line/Staggered: melee, ranged, siege/monks; Box: melee ring;
Flank: two halves), spaced by the largest member (Staggered 1.5×, as
`layout.rs:23-28`). Assignment: by class, then by projection on the facing and
lateral axes (today's `column.rs:5-31`), O(n log n) with few crossings.
Each slot is validated once against the clearance field (§3.6) and moved to
the nearest free point in a spiral of at most 8 cells.

### 3.3 Moving as a block with speed matching

- **Pace.** The anchor follows the shared route at the slowest member's speed
  [F A2, HG], down to 0.75× only while the rear lags more than `L` tiles. It
  never stops.
- **No assembly.** Member target =
  `route_point(anchor_progress) + rotate(slot, travel_heading) * squeeze`,
  `squeeze = min(1, clearance / half_width)` once per group per tick. A pass
  narrows the block (A2 *scaling*) to `max(1, floor(2 * clearance /
  diameter))` files; turns need no wheeling stop.
- **Catch-up.** Members may reach 1.25× pace, capped by their own speed (A2
  "temporary boost"); a far member heads for its moving target, so the block
  forms while walking [F 153015].
- **Short or scattered orders** (under 20 tiles [F 44725], or spread larger
  than the distance) skip the block: each member walks straight to its slot.
- **Members never pathfind.** One terrain check per step; if blocked, follow
  the group polyline with a clearance-clamped offset; after `K` blocked ticks
  become `Detached` with its own route via the fair scheduler, rejoining
  within 2 tiles. Replaces trail and rays (`steering.rs:63-125`).

### 3.4 Per-unit arrival and fighting

Anchor arrival stops nobody. Each member steers to `destination + slot` and
becomes `Arrived` on its own (`Idle`), so acquisition and retaliation apply at
once by stance; `acquisition.rs:84-86` becomes "member still Marching". The
group ends when no member is Marching. Marching members on a plain move do not
acquire [G, AoE2 convention].

### 3.5 Attack-move that resumes

Every 4 ticks, Marching members of an attack-move group run normal
acquisition (own LOS and stance, bounded scan) in a rotating window. A member
that finds a target becomes `Engaged` with `resume = group`; only it leaves
the block, and neighbours that see the enemy join, so 50 units no longer
funnel at one target. After `end_attack`, `resume` sends it back: Marching
with catch-up, else to its slot, else to the group destination. Queued legs
keep `attack_move`. The AI march uses `attack_move=True`. Client: an
attack-move cursor (`#scene[data-order-mode="attack-move"]`) and short-lived
destination markers (red attack-move, green move) with queued-waypoint flags.

### 3.6 Static clearance field

One `u8` per tile (land and water separately): Chebyshev distance to the
nearest impassable tile in half tiles, capped at 15. Lazy per 64×64 chunk
(4 KiB), invalidated by buildings and tree depletion; a 16k map's 256 Mi
tiles are never held eagerly. Used by §3.2, §3.3 and §4.3.

### 3.7 Partial re-orders, casualties, determinism

`break_formation(id)` becomes `leave_group(id)`: one member leaves, the group
keeps route, pace and slots (pace recomputed if the slowest left [F 99311]).
Death, new orders, Stop, retaliation and acquisition call it; a subset order
forms a new group; empty groups are deleted. Integer maths, canonical order by
group then unit id; the parallel phase reads only the snapshot.

## 4. Unit avoidance

### 4.1 How AoE2 units avoid each other

- **Model [F A1, A2].** A *hard* radius is never entered; a larger *soft*
  radius is preferred. Units predict positions; the lower-priority unit slows
  or stops; resolutions are remembered (no "merry-go-round"). A unit that
  cannot step aside is *pushed* by the higher-priority one; in a "stacked
  canyon" pushes chain and are undone after the group passes.
- **DE pushing [F].** Blocked units make blockers move aside (developer,
  2019: <https://steamcommunity.com/app/813780/discussions/0/2803982773091323561/>);
  44725 calls it "collide and correct".
- **Same-player overlap is real [F, secondary].** Recordings of live DE fights
  (third-party, genieutils + replay positions,
  <https://github.com/ddk220-light/aoe2_matchup/blob/HEAD/aoe2x/js_simulation/calibration/reports/hca_champion_overlap_suite_2026-08-13.md>)
  show friendly Champions normally 0.40 tile apart but moving ones down to
  0.003 tile (1st percentile 0.098), passing through friendly blockers while
  moving; the attack animation does not make allies passable. The same
  project's fitted engine allows stacks of two (rarely three) moving units
  and rejects a fourth [its model, not DE internals].
- **Formation allowance [F, forum, not developer].** Units in a formation
  treat other members (sometimes other formations) as having no collision
  size: <https://forums.ageofempires.com/t/nerf-teh-lancers/278913>.
- **Limits [F].** 185872 reduces stacking of overlapping melee groups; 153015
  stops villagers stacking on one activity; Stand Ground + Patrol stacking is a
  reported exploit, sometimes forced through a 1-tile corridor
  (<https://forums.ageofempires.com/t/please-fix-stacking-ranged-units-in-same-tile/194419>).
- **Collision radii [F].** Classic DAT (aoe2-model.md): infantry, archers,
  villagers 0.2 tile; cavalry 0.25; ram 0.45; ships, mangonel 0.5. DE DAT read
  by the same third-party project agrees: Champion 0.20, Paladin 0.25 (outline
  0.40), Elite Battle Elephant 0.25 (outline 0.50), Onager 0.50. Selection
  outlines are separate fields (<https://ugc.aoe2.rocks/general/attributes/attributes/>).
  81058 reduced Fishing Ship and Trade Cog collision, no numbers [F]; DE ship
  values are unconfirmed [G].
- **Head-on in a 1-tile corridor or bridge.** No source. 81058: units no
  longer stutter when colliding if the path has room, and path around
  obstructions for goals over 10 tiles; a 2023 roadmap lists groups failing
  narrow openings between large obstacles
  (<https://devtrackers.gg/aoe/p/6927feaa-looking-at-the-month-ahead-for-age-ii-de>).
  That friendly units squeeze past and enemies repath or wait is [G].
- **Ships [F].** 81058 (smaller hulls, nearest fish on depletion), 61321 (no
  stuck fishing ships when attacked), 111772 (no idle after a fish trap when
  spots are taken). A briefly blocked fishing ship goes idle (player report,
  <https://forums.ageofempires.com/t/fishing-ships-idle/62687>). No source on
  galleys passing in channels.
- **Melee and work spots.** 39284 hotfix improved melee pathing; 61321 stops
  melee on attack-move going idle; 81058 retargets a same-type unit when the
  bumped target is unreachable; 177723 improves large lumberjack groups;
  104954 stops villagers looping at a resource. No source gives fixed work
  spots or a melee cap; geometry (radius around the footprint) is the likely
  limit [G].

### 4.2 Current code (cause of S7)

Unit bodies are hard circles: `collision_sweep` rejects any overlap
(`movement_step/collision.rs:214-299`). The only relief is a same-player
squeeze after **64** blocked ticks, 3.2 s (`movement_step/settling.rs:20`), for
40 ticks (`:23`, `collision.rs:233-235`). Enemies and "settled workers"
(`Gathering`, `Fishing`, `Attacking`, ...) never pass (`collision.rs:152-164`,
`:248`).

- **Bridge:** a centre may sit at most `512 - r` off the deck axis
  (`collision.rs:356-398`); sidesteps are six fixed-angle tries
  (`movement_step/avoidance.rs:179-237`); different players never squeeze, so
  a head-on meeting can deadlock (plausible, not yet reproduced).
- **Narrow rivers:** ships use 512 for bodies *and* terrain
  (`collision.rs:175-181`); `body_position_clear` rejects a circle touching any
  non-water tile (`collision.rs:303-334`), so a 1-tile channel admits only the
  exact centreline, two ships never pass, and a fishing ship at work is a wall.
- **Villagers at resources:** spots are a diameter plus 16 apart on the
  footprint perimeter (`movement_step/slots.rs:14-36`); working villagers are
  walls; arrivals with over 32 reservations count as full
  (`spacing/destinations.rs`); cells with over 32 bodies refuse traffic
  (`collision.rs:8`, `:135`, `:277-297`); redirects only within 3 tiles.
- **Melee:** standing attackers are walls to friends; a melee unit needs a
  free perimeter point; enemy bodies are hard.

### 4.3 Avoidance design

Unit-unit contact becomes **separation with priority**, not a veto. Terrain
and buildings stay hard (existing exact sweeps, `static_collision.rs`).

1. **Proposal.** Every mover (group members included) proposes its desired
   step in the parallel `propose_steps_into`.
2. **Separation.** From the collision grid (at most 16 cells × 32 bodies), an
   integer separation vector over neighbours within soft radius `r_a + r_b`.
   Overlap is allowed down to a hard distance `h * (r_a + r_b)` and costs
   speed linearly. `h` [G, tune]: 0 same group (A forum allowance), 0.5 same
   player moving, 0.75 same player versus a stationary worker, 0.85 allied
   or enemy. At most two bodies may overlap one point (fitted DE behaviour);
   a third is pushed away.
3. **Priority and pushing [A1, DE].** Movers outrank arrived, idle and working
   units; ties by id. The lower unit absorbs the push. A working unit is
   nudged at most 0.15 tile and springs back to its spot.
4. **Apply.** Jacobi-style from the start-of-tick snapshot, in parallel; the
   ordered apply keeps only the static sweep and bucket/vision writes. The
   step is clamped so `h` is never violated; nothing waits 64 ticks.
5. **Head-on and bridges.** Opposing movers each bear right of their own
   heading, separating laterally first. On a 1-tile bridge two 0.2 bodies
   pass with centres 0.2 tile off-axis (legal: `512 - 205 = 307`). Larger
   bodies, and enemies, overlap down to `h` at reduced speed: never a
   deadlock, never a tunnel through a separated body.
6. **Narrow water.** Ships get a terrain radius of 0.25 tile, separate from
   the 0.5 body radius, so one ship sails any 1-tile channel and two pass in 2
   tiles. Working fishing ships are soft (`h` 0.5 to friendly ships); a
   blocked fishing ship keeps its job rather than going idle.
7. **Villagers at resources.** Perimeter spots (`slots.rs`) become
   preferences: work starts within reach of the footprint even if the spot is
   taken, with at most two bodies per point. Working villagers are soft to
   passing friends. The overfull-cell and over-32-reservation vetoes become a
   density cost, so a crowded mine sends late arrivals to the next free side.
8. **Melee in crowds.** A melee unit within `attack_range + r` of the target
   surface strikes even if its perimeter point is taken; friendly attackers
   are soft; enemies obey `h` 0.85, keeping DE's limited melee stacking.
9. **Removed.** The follower speed-0 rule (`spacing.rs:37-106` and
   `spacing/followers*`), the 64-tick squeeze, the idle push
   (`spacing.rs:122-209`) and most of the sidestep and refinement machinery
   for unit (not static) blockers (`avoidance.rs:18-103`, `:179-237`).

## 5. Performance

### 5.1 Measured (owner, 100-player match)

Median 16 ticks/s against a 20 Hz target, with AIs far below pop cap. At about
11k units: tick 53 ms p50; movement about 40 ms p50 and 54 ms p95; planning
about 13 ms p50. Not reproduced in this study.

### 5.2 Where movement time goes in the current code

Read from code; the split between items is unmeasured. PR 0 reports it via the
existing probe spans (`avoidance_query`, `sidestep`, `arrival_search`,
`local_refinement`).

| Cost | Location |
| --- | --- |
| Apply re-runs the follower probe, the arrival check and the full swept collision test **sequentially** for every proposal | `movement_step/mod.rs:376-383` |
| Any blocked, obstructed (≥ 16 ticks), arriving, multi-waypoint or group mover is refused by the parallel phase | `movement_step/mod.rs:206-216`, `:229-231`, `:244-253`, `:265-267`, `:305-308` |
| The refused run the sequential loop in 512-subunit sub-steps, each with a swept sweep, up to six sidestep sweeps and terrain checks | `game_movement.rs:110-180`, `avoidance.rs:18-103`, `:179-237` |
| Vision is updated every sub-step in that loop, unconditionally | `game_movement.rs:177` |
| Each sweep scans up to 16 cells × 32 bodies with a cross product per body | `collision.rs:114-147`, `:214-299` |
| Each step relocates the collision, follower or idle and wildlife indexes (hash maps) | `wildlife.rs:251-280` |
| Per-mover sequential `settle_obstructed_unit` and `replace_occupied_arrival` (BTreeMap destination scans) | `game_movement.rs:68-88`, `spacing/destinations.rs` |
| Group members run fully sequentially, with up to 96-tile rays and cache churn | `formation/steering.rs:16-193`, `:237` |
| Local searches (306 nodes) and the lookahead pass over every mover | `movement_step/refinement.rs:190-215`, `movement_step/lookahead.rs:11-46` |
| Planning (13 ms) is mostly per-unit searches: route budgets per tick and per player | `game_movement.rs:11-32`, `:45` |

### 5.3 How the design gets under budget

- **Parallel, single-pass movement.** Every mover is proposed in parallel;
  separation reads the snapshot, so apply re-checks only the static sweep.
  This removes the sequential collision re-test and follower probe
  (`mod.rs:376-383`), the sequential fallback for blocked units and the
  per-sub-step vision update (gated by `unit_vision_stale`, as the fast path
  already is).
- **Cheaper neighbour reads.** Separation needs one bounded neighbour pass per
  unit, not up to seven swept tests. A flat per-tile grid (sorted
  struct-of-arrays rebuilt per tick, or dirty-cell updates) replaces hash-map
  relocations on the hot path.
- **No member searches or rays.** One route and one clearance lookup per
  group; Detached members only. Fewer planner calls cut planning, because the
  AI's armies march as groups.
- **Sleeping units.** Arrived and idle units drop out of the mover list and are
  only touched when pushed.

| Metric (native, shared 6-slot dev host) | Today | Target |
| --- | --- | --- |
| Tick rate, 100-player match | 16/s median | **20/s sustained** |
| Whole tick p50 / p95 at ~11k units | 53 ms / n.m. | ≤ 30 / ≤ 45 ms |
| Movement p50 / p95 at ~11k units | ~40 / ~54 ms | **≤ 10 / ≤ 20 ms** |
| Planning p50 | ~13 ms | ≤ 6 ms |
| Movement p50, 15k movers | n.m. | ≤ 14 ms |
| Per-mover movement cost, single-thread equivalent | ~3.6 µs | ≤ 1.5 µs |
| Sequential work per mover in apply | sweep + follower probe + arrival check | static sweep only |
| 100 units, 40 open tiles: last arrival after the leader | unbounded | ≤ 3 s |

PR 0 adds `perf-groups` (11k movers in 200 armies of 30–100, crossings, a
bridge, a 2-tile pass, a crowded mine) and a 100-player replay workload. It
reports p50/p95, instructions and allocations on one source and machine, never
lowering a baseline. Synthetic workloads do not qualify a finished RTS or
dedicated hardware; the 100-player match is the acceptance run.

## 6. Implementation plan (small, shippable PRs)

Each PR changes behaviour in place and adds one scenario test that fails
before it. It deletes the tests it makes obsolete (net negative where
possible), runs the focused gate and `make preflight` (plus `make test-e2e`
for client changes), and ships a before/after scene of its own change
(`make compare` or a showcase video).

- **PR 0: baseline (tests and perf only).**
  - Scenarios: 60 militia in Line on open ground; a 2-tile forest pass;
    re-ordering 10 of 60; attack-move into 10 enemies; a two-player head-on on
    a 1-tile bridge; 4 fishing ships in a 1-tile river; 12 villagers on one
    gold mine; 40 vs 40 melee. Each is recorded as an expected-fail metric.
  - `perf-groups` with the §5.2 span split.
- **PR 1: leaving a group is per unit (S4).** `leave_group` at
  `formation/mod.rs:72`, `orders.rs:54`, `order.rs:61` and `:88`,
  `lifecycle.rs:55` and `acquisition.rs:130`.
  - Test: re-order 10 of 60 and kill 1; the other 49 arrive. Replaces
    `pressure.rs:143`.
- **PR 2: attack-move indicator (client).**
  - Test: e2e check of the cursor and marker.
- **PR 3: per-unit arrival; fighting on arrival (S3, S6).**
  - Test: with one member stuck, the rest arrive idle at their slots, and the
    first arrivals attack before the last arrives.
- **PR 4: per-member attack-move that resumes; the AI uses it (S5, S6).**
  - Test: attack-move through 10 enemies kills them all, then at least 95 % of
    survivors reach their slots.
- **PR 5: clearance field, no behaviour change (§3.6).**
  - Test: it matches a brute-force reference, and a new building updates it.
- **PR 6: destination slots and block movement; the column is deleted (S1,
  S2).** Members join the parallel phase. Deletes `column.rs`, the trail, the
  rays and `arrival.rs` swaps.
  - Test: last arrival within 3 s of the leader on open ground.
  - Test: the 2-tile pass takes at most 1.6× the open-ground time.
- **PR 7: soft separation and pushing (§4.3 steps 1–4, 9) (S2, S7).** The
  follower stop and the 64-tick squeeze are removed.
  - Test: two crossing groups of 40 arrive with no unit frozen for more than
    1 s, and no pair ever breaks `h`.
- **PR 8: head-on and bridges (step 5).**
  - Test: two players' units meeting on a 1-tile bridge both cross within
    3 s.
- **PR 9: narrow water (step 6).**
  - Test: 4 fishing ships work in a 1-tile river, and 2 galleys pass in a
    2-tile river.
- **PR 10: villagers and melee crowds (steps 7–8).**
  - Test: 12 villagers all gather at one mine within 10 s.
  - Test: in 40 vs 40, at least 90 % of melee units strike within 5 s.
- **PR 11: performance closure (§5.3).** Single-pass apply, a flat grid,
  sleeping units and gated vision.
  - Acceptance: the §5.3 targets on `perf-groups` and the 100-player replay,
    reported with revision, commands and limits.

PRs 1–4 fix the most visible complaints without the refactor and ship first,
one client branch at a time. PR 6 needs PR 5, and PRs 7–11 build on PR 6.

## 7. Open questions

- Should plain-move members hit back at melee range while marching? AoE2
  suggests not [G]; §3.4 already gives fighting on arrival.
- `h`, the two-body stack limit, the 1.25 catch-up, `L` and `K` are AoeWorld
  choices [G]. Tune them on the scenario metrics, not by feel.
- DE ship radii and head-on corridor behaviour are unpublished [G]; PRs 8–9
  are judged on their scenarios.

## 8. Measured baseline and PR 6 results

At 20 ticks/s, `tests/group_baseline/` pins current output exactly. Before is
PRs 0–4; PR 5 (`fe8586ee`) leaves movement unchanged. PR 6 requires every
member at its assigned destination: a give-up cannot pass as arrival.

| Scenario | Before | PR 6 / remaining target |
| --- | --- | --- |
| 60 militia, Line, 100 open tiles | first stop tick 2159, last 2328 (+169 ticks); no column; never longer than 10 tiles | last within 60 ticks of first (6) |
| Same, 6-tile forest with a 2-tile pass | first 2238, last 3955 (1.70× open); 0 stop over 8 tiles short (PR 3; was 14); single file for 1,737 ticks, 78 tiles long | ≤ 1.6× (6) |
| 30 militia and one sealed in, enemy 3 tiles past the goal | (PR 3) the 30 reach the goal between ticks 2533 and 3019; first strike on the enemy at 2572, before the last arrival and before the sealed unit gives up at 2753; before PR 3 the first strike came at 3082 | kept |
| Re-order 10 of 60 | (before PR 1) the other 50 left the group, stopped at once, none arrived; now all 50 stay grouped and arrive | all 50 arrive (1) |
| Attack-move, 60 vs 10 | (PR 4) all 10 die by tick 1366; all 60 survive and stand within a tile of their own slot; with the first PR 4 look (13x13 tiles) 1564 and 56; before: 8 of 10 died, 55 idle near x 75 of 140, none at the goal | kept (4) |
| Attack-move, 24 through 5 standing ground (the PR 4 showcase) | (PR 4) members break off within their own 4-tile sight from tick 101; 5 s later 13 fight and the other 11 march at the group's pace; no unit stands in the attack pose out of reach; with the 13x13-tile look: from tick 37, up to 8.4 tiles away, 19 broken off 5 s later and 13 of them standing out of reach (6,871 member-ticks) | kept (4) |
| Head-on, 3 vs 3, bridge | no deadlock: first crossing tick 181, all across 251 | both cross within 3 s (8) |
| 4 fishing ships, 1-tile river | all 4 gather orders refused; 3-tile river: 2 refused | all 4 work (9) |
| 12 villagers, one gold mine | 2 carry gold by 10 s, 9 by 30 s, 10 by 60 s; 2 idle | all 12 by 10 s (10) |
| 40 vs 40 melee | (PR 4, members engage one by one, only for an enemy in sight with a free spot) first blow tick 234; 36 distinct strikers in 5 s (lower bound), 79 in 60 s; 2 and 7 alive; with the 13x13-tile look: tick 270, 6 and 64, 21 and 14; before PR 4: tick 304, 12 and 36, 29 and 26 alive | ≥ 90 % strike in 5 s (10) |

Matched `make perf-groups`: 12-CPU slot, shared host, fog on, 200 Line armies
(30–79 units), 10,900 units, 100 warm-up ticks, 300 ticks/window. Before is
clean `fe8586ee`; initial PR 6 is `1b5a3aa6`. Optimization keeps every scenario
result above and the canonical replay pins unchanged.

| Plain window (ms, p50 / p95) | Before PR 6 | Initial PR 6 | Optimized |
| --- | --- | --- | --- |
| Movement | 4.895 / 5.254 | 11.104 / 12.328 | **4.835 / 5.756** |
| Whole tick | 5.100 / 5.482 | 11.792 / 13.097 | 5.507 / 6.407 |

PR 3 and PR 4 as built: a member that cannot move for 240 ticks after the
anchor arrived stops where it stands; the column is dropped at anchor arrival;
every 4 ticks an attack-move group makes one bounded look over its extent
(at most 2,048 bucket entries and 32 enemies); a member breaks off only for an
enemy within its own sight that has a free melee spot (or is in reach), at most
eight per look; a unit that chose its own target (acquisition, attack-move)
gives it up when every spot around it is taken instead of standing out of
reach, and the client draws the attack pose only with the target in reach.
Members fight alone and march on
as a one-unit attack-move group to their slot at the group destination (a
shortcut: the design's `Engaged` state inside the group needs PR 6's group
ids, because `stop_movement` still calls `leave_group`). A group leader's
obstruction no longer triggers the 240-tick give-up, which had ended the whole
group's route. The remaining legs of a queued order are not resumed by a leaver.

The initial regression came chiefly from repeated hard-body sweeps, not slot
assignment or the clearance field. Movement-only Callgrind falls from
96,413,953 to 44,116,679 instructions. The optimized p50 meets 4.9 ms; the p95
remains above the historical PR 5 result. See [profiling](group-profiling.md)
for reproduction, the fresh PR 6 comparison, source costs, phase/span splits,
activity census and validation. Spans overlap and include instrumentation cost;
the later profiled window has different activity from the plain window.
These open synthetic scenes do not qualify RTS performance or dedicated
hardware; PRs 7/11 remain necessary for crowding/performance closure.

Implementation evidence and differences from the design:
- S1 never formed a column on open ground; the obstacle caused it. Column
  mode is now deleted; the replacement geometry counter requires >20 tiles
  of stretch and frontage below one infantry diameter. It stays zero.
- Hard bodies remain until PR 7. Open slots leave a passing lane plus a
  snapshot step; narrow files need only a diameter plus that step. A half-tile
  lag slows the anchor to 0.75×; catch-up is exactly 1.25×, capped by own speed.
  Initial offsets blend toward the fixed destination layout while walking.
- Five clearance samples cover eight tiles ahead/behind. Static obstruction
  for 120 ticks permits at most four detached scheduler requests per tick;
  members keep their slot/group and rejoin within two tiles. Sealed members
  can move inside their tile after ray removal, then give up independently.
- Scheduler storage still belongs to a member, with route handover when it
  leaves/detaches. Independent group ids / in-group Engaged remain absent:
  PR 4 one-unit resumes stay; a leaver still loses old queued legs.
