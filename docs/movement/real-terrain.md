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

This milestone pins river arrival at 6,035 ticks (4,742 ideal), maximum stall
26 and five oscillations; General arrival at 40,838 ticks (34,440 ideal), maximum
stall 149 and 37 oscillations. Every unit reaches its exact slot without a speed
or membership violation. The next milestone retains the three-second/zero-
oscillation target; these two deficits are explicit regression pins, not a
claim of complete movement acceptance. Forest, town and the other terrain
cases retain the full strict assertions. Synthetic cases do not qualify RTS
performance on dedicated hardware.

The cohesion floor is 7/8 of the slowest member's pace. The former 3/4 floor
alone could exceed the 1.3x arrival target. Current output pins are refreshed
with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation
AOE_NATIVE_TEST_FILTER='test(group_baseline::)'`: earlier arrivals and contact
change the timestamps and battle census. Sight, hit reach, participation,
spacing, no-overlap and work ceilings are unchanged.
