# Movement across real terrain

Objective, in priority order:

1. Every unit with a physically reachable destination makes progress. Temporary
   traffic must not become deadlock or livelock; stopping short is failure.
2. Minimize the last member's arrival time relative to the slowest member's
   shortest clearance-aware route. The initial acceptance target is 1.3 times
   that reference, with no pending member stationary for more than 60 ticks
   (three seconds at 20 Hz).
3. Preserve shape where it fits, compress through obstacles, and recover the
   assigned shape after the squeeze. Do not send an advanced member backwards
   merely to fill the rear of a marching file.
4. Retain avoidance decisions to prevent repeated left/right oscillation.
5. Bound work per tick independently of march distance. Performance acceptance
   also requires the unchanged instruction gate, not just successful arrival.

These priorities do not promise passage through sealed ground, or permit a
large body to cross a physically narrower passage. All decisions use integer
arithmetic and canonical entity/coordinate tie breaks.

## Route and clearance

Use the existing retained hierarchical planner and fair, bounded scheduler.
One group owns one route. Route admission and continuation must use a hull
large enough for every member, including siege; the first member's hull is
not a valid substitute. Detailed edges must use the same swept static circle
predicate as movement. Foundation footprints count from placement, inflated
by the actor's radius. Cached routes must distinguish the clearance class and
obstacle revision. Broad hierarchy connectivity guides search but cannot
certify a narrow detailed edge for a larger hull.

Bridge decks are continuous corridors, with radius inset from their polygon
edges. Grid nodes may guide traversal, but neither diagonal tile-center
corners nor a passable endpoint alone prove a swept edge safe. Use the same
corridor predicate for planning, local refinement and movement.

## Shared corridor and slots

Keep the shared polyline parameterized by integer arc distance. Each member
keeps monotone progress and a local segment cursor. Steer toward a nearby
point on that polyline, with an offset clamped to the corridor's clearance,
instead of cutting across a river bend toward a distant anchor-relative slot.
Use the current block target while it is locally reachable; terrain fallback
must follow the corridor, including a turn away from the final destination.

Compress according to the available width and largest diameter. Assign files
by spatial proximity and forward order; retain canonical member storage.
Reassignment happens on a squeeze transition, not on every unit step. Blend
back to destination offsets after the rear clears the obstacle. Arrival is
per member at its assigned reachable slot; anchor arrival is not group arrival.
A timed-out or idle member short of its slot never counts as an arrival.
The army table measures this: a soldier counts only within two tiles of the
slot it was last assigned (the occupied-slot exception), not anywhere near the goal.

## Local traffic and recovery

Use bounded spatial queries and swept integer circles. For reciprocal head-on
traffic choose the same passing convention relative to each unit's direction;
retain the choice across ticks. Stable entity priority resolves simultaneous
claims; yielding must preserve the original goal. Idle friendly soldiers may
yield, but active workers and enemies keep their collision boundaries.

Escalate sustained failure: committed lateral steering, bounded local replan,
then a separately scheduled detached route and rejoin. The existing temporary
same-player squeeze is a last resort, not static-terrain ghosting. No new
randomness, wall clock, unbounded neighbour scan or per-member global search
in the normal marching loop.

## A player's order beats a General

A General's army never overrides the player. The whole rule, implemented once
in `general_player_order` (`economy/general/orders.rs`):

1. Per player, a soldier is in an army or detached.
2. Any player order that names a soldier (move, attack, stop, garrison, load,
   hunt, formation, enabling auto-scout) detaches it and restarts its clock,
   whether or not any army exists or it ever belonged to one (villagers are
   not soldiers and are never recorded; the set is bounded by living soldiers
   and cleared on death: 5,600 entries when every soldier of the 100-seat
   late-game workload is ordered). A formation order that also names the
   soldier's General detaches the soldier too: it follows the order it was
   given, and only the unnamed rest of the army follows his march.
3. Detachment survives garrison (a sheltered soldier has no map record but
   keeps its entry).
4. Only a player's order that names his own General (move, stop, move-alone,
   anything)
   readmits detached soldiers, and only those that are idle (not moving,
   planning, attacking or auto-scouting), within 24 tiles and at least 300
   ticks past their last order. Automatic marches (vassal defence recall) never
   readmit.
5. A soldier under a live player order is never touched; another player's
   General never takes it.
6. The detached set and its clocks are in the state hash.

## Group or individual: the current rule

Measured on `tests/real_terrain/armies`, carved river, forest and pass shapes;
`.cache/armies/enabled` names a run of its on-demand table.

1. A group paths as one: the anchor owns one route and members follow its corridor,
   packing into files where clearance requires.
2. A member blocked for 120 ticks, or stalled for 60 after the anchor arrived,
   breaks off: it searches its own route to a corridor point 8 tiles ahead, or to
   its slot. Group membership and slot survive.
3. On a deck, or after 64 blocked ticks on one-lane ground, a blocked member of any
   group passes friendly bodies: chokepoints are crossed in sequence without a knot.
4. A detour ends when it reaches its goal, when its body is blocked for 240 ticks,
   or when a search from a deck edge has not answered in 240 ticks.
5. After the anchor arrived, a member that stopped far from its slot asks for a new
   route every 256 ticks, at most 8 times. A member beside a slot that a stationary
   body took has arrived there.

Not done: the anchor still runs ahead of a queue, so the shape re-forms at the
destination rather than after each chokepoint.

## Measurements and implementation sequence

Add tests before changing production behaviour. Fixtures must exercise mixed
infantry/archers/cavalry/siege (40), river detours, forest gaps, buildings and
farms, opposing formations, a 100-member General march over roughly 1000 tiles,
a diagonal off-grid bridge, a siege foundation detour and a one-tile head-on
gap. Prefer typed generated map fixtures; label synthetic obstacle masks as
such. They are regression workloads, not dedicated hardware qualification.

Record exact-slot arrivals, last arrival tick, ideal route ticks, maximum
stationary interval while pending, significant direction reversals, final
slot error, route work and instrumented instruction counts. Count an oscillation
as repeated direction reversals within 60 ticks, each after half a tile;
a single deliberate obstacle turn and quantization jitter are not oscillation.
Keep baseline measurements in the report; acceptance tests assert working
thresholds rather than preserving broken output.

Milestones:

1. Reproduce a mixed river detour; repair shared-corridor following and verify
   unchanged simulation tests. Capture the same scene before and after.
2. Extend the suite to forest/town/crossing groups and clearance-aware hull
   routing. Measure compress/reform and spatial slot assignment.
3. Complete bridge/foundation/head-on and long General march scenarios;
   profile bounded tick work and optimize measured hot paths.
4. Run full focused simulation, affected server pins, lint, structure, hooks,
   preflight and perf-ci. Record real-client footage of all requested scenes.

Every visible-change commit needs before/after real-client capture using the
real asset pack. Local capture setup stays in `.cache/showcase`; assets remain
outside Git and images. Update `reports/movement/real-terrain.json` after each
commit with exact revision, commands, metrics, videos and unresolved limits.
Do not mark the report complete while any acceptance or capture is missing.

## Current experiments and evidence

The initial eight fixtures reproduce failures on `21cb523d`. The river leaves
one of 40 units short; the diagonal bridge leaves both villagers short; the
long march leaves eight of 100 short at the 75,000-tick bound. Forest and town
arrive but exceed the stall/oscillation limits. The local report retains these
measurements and the subsequent unsuccessful trials.

The saved corridor prototype retains bounded route continuations ahead of the
anchor and smooths the polyline's adjacent tangents. A member records forward
arc progress, follows the corridor after static obstruction, and uses a short
eight-tile recovery connection. It rejoins before the movement snapshot so a
recovery cannot spend both ordinary and formation movement credit in one tick.
Recovery keeps the original group and exact destination slot. Final-slot
steering begins on the final approach, including slots beyond the centroid.
Occupied-waypoint deferral resets segment progress to the physical body.

A bank-hugging route also needs a useful cross-section: circular clearance at
one bank understates a wide ford. The saved experiment probes at most 32
half-tile steps on either side, through connected swept clearance for the
largest member, and centers packed files at the nearest maximum-clearance
point. Every physical movement proposal retains body and static checks. This
experiment failed existing formation checks and stays in the local checkpoint.
It must be revised before inclusion in a movement commit.

For raster bridges, continuous deck strips are inset by the body's radius.
The swept-center segment is clipped against neighbouring strips using rational
integer intervals; their union must cover the whole step in water. This closes
the inside-corner chord hole that endpoint-only support admits. The integer
square-root inset is conservative. The existing tile connectivity remains the
global search graph; fractional physical steps use the continuous support test.

Deferred occupied waypoints must not bank travel. When a physical proposal is
skipped and route continuation waits for search work, reset the unpaid segment
to the actual body. Retain the search if its source tile is unchanged; otherwise
discard it and replan. The nearby food-worker bound remains 179 ticks.

## River corridor iteration

Members that meet static terrain retain forward corridor following instead of
returning to an anchor-relative chord on the next tick. A member parked at a
temporary assembly target also advances along that corridor; it does not wait
for a virtual row to catch up. Destination slots stay fixed. Final approach
starts at each slot's projection on the last route leg, so rear ranks do not
first pass their own destinations.

The anchor emits already proven continuations into a FIFO bounded by three
normal route segments. Prefetch spends zero search work; fair lookahead still
owns searches. Refresh the shared future polyline when that FIFO changes.
Persistent outliers request an eight-tile corridor connection, retaining their
membership and final slot. Rejoin before either movement snapshot, preventing
an ordinary step and a group step in the same tick. Detached recovery uses the
ordinary bounded friendly squeeze rule after sustained obstruction, with the
same hard distances and terrain vetoes.

Ordinary occupied-arrival replacement must skip grouped recovery movers: its
new move order would release the member and discard the original final slot.
A reserved recovery endpoint stays under corridor rejoin/recovery ownership.
The full late-game replay now checks the instruction kernel's unchanged 1,200
formation-member population, alongside the one-/four-worker equality check.

The forty-member river pin is now 5,436 ticks (4,742 ideal), maximum stall 10
and three oscillations; the thousand-tile General march 37,023 ticks (34,440
ideal), maximum stall 24 and 21 oscillations (formerly 6,035/26/5 and
40,838/149/37). Every unit reaches its exact slot without a speed or
membership violation. The three-second/zero-oscillation target still applies
to these two; their deficits are explicit regression pins, not a claim of
complete movement acceptance. Forest, town and the other terrain cases retain
the full strict assertions. Synthetic cases do not qualify RTS performance on
dedicated hardware.

## General march iteration

Measured on the 100-follower river fixture before this change: the planner's
greedy legs ran east to the bank and then along it (163 tiles, 1.30 times the
oracle route); every look-ahead sample on that bank-hugging route had one half
tile of clearance, so the block packed into one file for 3,100 ticks; and the
all-or-nothing cohesion check held the anchor at 7/8 pace on every tick.

The anchor now shapes its known route whenever its end changes. Each vertex
moves perpendicular to its neighbours' chord, through crossable tiles, to the
nearest tile within seven tiles that has 5.5 tiles of clearance (or the most
available). A greedy any-angle pull then keeps the farthest moved vertex whose
straight edge never comes closer to static obstacles than the vertices it
replaces, sampling every half tile with the anchor's own tile-crossing checks.
The route's last vertex stays exact for the planner continuation; a dead end
keeps the planner route. Work is at most 4,096 + 128 per member static lookups
per change, charged as group collision work. Route continuations are fetched
while the known end is within 64 tiles, so the corridor sees a ford before the
block reaches the bank.

Pace keeps a cohesion reserve of up to 1/8 in 64ths. On the march it follows
the corridor lag of the rearmost tenth of pending members: nothing within two
tiles, the full reserve from ten. Within a block length of the destination the
rearmost member counts, including its remaining distance to the slot, from
one tile to five, so the ranks still arrive within three seconds. A destination
packed inside a passage keeps the strict half-tile rule, because files cannot
reorder there. Trailing members catch up at up to 1.5 times the pace. Squeeze
re-ranking assigns files in lateral order and rows by forward order inside each
file, so a narrowing block closes towards its centre instead of shuffling
members across files.

The three 100-follower General fixtures now meet the 1.3x target with every
follower at its exact slot: river 5,906/4,646 ticks (was 7,495), forest
3,775/3,130 (was 4,216) and far bank 5,047/4,152 (was 6,270), with maximum
stalls of 31, 13 and 31 ticks (were 63, 27 and 85). They still record five,
three and three short direction reversals, mostly while the block first forms;
the General tests assert arrival, slots and stalls, not zero oscillation.

Current output pins are refreshed with `make test-native-focused
AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
earlier arrivals and contact change the timestamps and battle census. Sight,
hit reach, participation, spacing, no-overlap and work ceilings are unchanged.

## Single-lane bridges

The owner's 100-player games showed armies stuck at one-lane bridges: the
front crossed, the rest stayed on the shore; two villagers and a Knight once
held a deck against everyone. `bridge_march` reproduces this with typed
hydrology and wood nodes shaped after generated-map crossings: a straight
deck reached along a river strip (Paris-like), a 45-degree deck whose mouths
are three-tile lanes through trees, and a 650-tile march over a straight, an
off-grid (2:1) and a diagonal bridge between forest belts. Each runs a General
with 100 followers and a 40-unit mixed formation; two formations also meet
head-on at the straight and the wooded deck, and two villagers meet a Knight
on the straight deck. The ideal adds the slowest unit's route to the time
every commanded body needs to file over the deck at the slowest speed.

Movement rules, all bounded and integer:

- A body over a water corner beside a diagonal deck is not sealed in: the
  continuous deck carries it (`group_tile_sealed`). A body pressed off a deck
  edge may only step closer to the deck's centre line (`deck_recovery_step`).
- A member earns arc progress only when it actually moved; a blocked body
  beside a bend no longer runs its progress (and its target) tiles ahead.
  Near a one-lane narrowing, including the body's own tile, a member first
  steps sideways onto its lane at its own progress.
- A packed block keeps its narrow files until its rearmost pending member has
  left a bridge deck, sampled back along the corridor.
- On a bridge deck, a single file's own members blocked for 16 ticks squeeze
  to the friendly hard distance and pass through each other; the forest-pass
  no-stacking rule stays strict elsewhere.
- Oncoming same-player movers that stay blocked pass through each other on a
  deck after 48 ticks, and in a passage at most one tile wide after 16 ticks
  when they belong to different groups. Workers, enemies and same-direction
  traffic keep their collision boundaries.

Ordinary planning keeps one shared route; no member re-plans a long detour.
The straightened corridor uses the group's largest body, and a slot cut off
from the destination by water is pulled back to the last directly reachable
point. Results are in the commit messages; real-map runs on the Paris and
France packages are local measurements, not CI tests.
