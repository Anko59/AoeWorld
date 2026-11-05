# Steward

The Steward is an original AoeWorld unit using the licensed King graphic.
A completed Town Center in Castle Age trains one for 1,000 food and 1,000 gold
in 60 game seconds, adding one living unit. A queued Steward reserves the
Town Center too. Cancellation refunds the full cost; death permits a replacement.

A Steward can move freely and shelter in any completed owned Town Center without
another Steward. Occupants, queued training and incoming garrison orders reserve
the single Steward place. Cancelling an arrival frees that reservation. Leaving
releases its domain and workers immediately, and the old Town Center can train a
replacement. Outside a Town Center it manages nothing, including after destruction.
Settings travel with the Steward when it occupies a different Town Center.
Roaming Stewards can board transports through the ordinary shore rendezvous.

There is no global population ceiling. Completed Houses, Castles and
Town Centers provide space; training waits when that space is full.
[World capacities](../scale/capacity.md) records the matching storage bounds
and the distinction between capacity and performance qualification.

## Deterministic economy

Sheltering the Steward in a Town Center activates its circular domain. The Rust simulation
visits up to eight active domains every 20 ticks (one second at 20 Hz), rotating
in entity order. Each domain scans at most 1,024 spatial-bucket entries and 2,048
of its owner's buildings; a retained cursor progresses through crowded buckets.
A partial census never authorizes training or a new foundation. Each admitted
domain may attempt 64 ordinary orders, eight bounded resource searches and 32
placement checks. Site searches resume across passes. The world maxima per pass
are therefore 8,192 unit visits, 16,384 building visits, 512 orders, 64 resource
searches and 256 site checks. Ordinary pathfinding retains its shared budgets.

### Exact rules (what is adopted, when TCs train, why it stops)

**Territory.** A Steward's territory is the 100-tile circle around its Town Center,
cut by its sibling Stewards: every point belongs to the nearest *active* Town Center
of the same player (integer squared distance, ties to the lower Steward ID), so
no villager, resource node or building ever has two Stewards. A Town Center is active
while its Steward is garrisoned in it; an inactive Town Center claims nothing and its
neighbour takes its area at the next pass. A building belongs to the Steward
whose territory holds its centre, and must also lie wholly in the circle.

**Adoption.** Each pass a Steward scans its territory and adopts, in entity
order, up to 100 of the owner's villagers: busy ones (gathering, farming,
building) are taken over in place when their existing job also belongs to
this domain; idle ones are given work by the job pass. A worker crossing a
neighbour's territory to serve its farm, resource or building keeps that job
without being adopted there. It can be adopted when it enters the job's domain.
Villagers under a manual order (plain move, garrison order)
are left alone until idle again, and an accepted manual order releases a worker
at once. Soldiers, foreign units and units outside the territory are never
adopted.

**Training.** Training starts OFF. The selected occupied Town Center exposes a
training switch, four resource-mix sliders (0–100, at least one positive), and
a villager-cap slider (0–100, default 100). These settings belong to its Steward
and are confirmed by the server. When training is on, each complete Town Center in the territory with an empty queue
trains one villager per pass (within the original two-order support window per Steward) when the player
has 50 food, until residents (every living villager in the territory, sheltered
ones included) plus queued villagers reach the chosen cap. It stops, and the HUD says why,
when: the census is incomplete (more than 1,024 units in the circle),
residents+queued reaches the chosen cap, no Town Center lies in the territory,
food is short, or housing is full. The Steward builds Houses before population
plus queued training fills the player's space, including when its workers are
already busy.

**Containment.** A Steward only gives a villager a job (resource, Farm, foundation,
new building site, replacement target) when a walk exists that stays inside its
own territory. Static ground (water, cliffs, bridges) of the territory is labelled
into components once per territory or bridge change; each check is a few array
reads, never a route search. A villager with no reachable work stays Idle.
Ships are not restricted.

**Stalls.** Every pass the Steward checks its managed workers. One whose task is
Moving but whose position has not changed for 8 seconds is stopped and
re-tasked, avoiding the node it was heading to (after a second consecutive
stall, avoiding that resource kind). Unmanaged villagers of the territory that
are Moving without progress get the same Stop, so they become idle and are adopted. The client treats a villager reported as
Moving that is neither moving nor planning as idle for the idle-villager count.

The HUD line for a selected Town Center reads "n/100 villagers managed · r in
territory · reason", with the counts coming from the server (the old text
counted only units inside the camera subscription).

Budgets and limits:

The selected villager cap (at most 100) counts living residents, including manual workers and
sheltered villagers, plus queued villagers in every domain Town Center. Training
uses complete TCs with their spawn perimeter and rally inside the domain; it
respects the player's food. Deaths permit
replacements. New adoptions stop at the selected cap.
When the owner lowers the cap, excess managed villagers move to the nearest
active Steward with room. Idle villagers that cannot find local work may also
transfer. Workers stay in place when no other Steward has room. Ordinary
movement, order budgets and territory adoption still apply.

Jobs use the kingdom's shared stocks and actual deliveries, with default
food:wood:gold:stone weights of 45:30:15:10. The lowest weighted projected
reserve gets priority: current stock plus deliveries in the recent minute of
game time. Comparisons use integer cross-products. Local worker counts do not
stand in for income. Each player retains twelve five-second delivery buckets;
ordinary resource returns and carried loads deposited on garrison entry count
only the amount actually credited. Purchases, spending and refunds do not
create harvested income. Every bucket's epoch and amounts participate in the
canonical hash, including income earned before a Steward becomes active.

Idle workers and unloaded managed gatherers respond to those kingdom needs.
Carried loads finish their ordinary delivery first, and a refused replacement
keeps the current productive job. Training retains its normal admission before
productive reassignment; optional maintenance follows.
Each domain pass adjusts at most one productive job for a moderate imbalance.
Further changes require the new weighted projected reserve to be below one
quarter of the current resource's reserve. Idle dispatch keeps its ordinary
budget, and severe shortages can still redirect the available workforce.
Known resources within ten tiles are preferred. A local miss can also probe a
completed matching drop-off, rotating its building ID between queries. A
retained search visits eight-tile blocks outward from the domain centre, so
Gold and Stone beyond the ordinary ten-tile replacement radius remain usable.
The ordinary replacement search itself keeps its existing radius.

Distant drop-offs prompt normal Lumber/Mining Camp or Mill construction. An
existing eligible distant camp remains usable when a nearer one is
unaffordable; a worker is not sent to harvest a load with no usable drop-off or
admitted camp. When Food work is chosen, farmers reclaim a free paid-for Farm before
opening another plot. Each unfinished Farm keeps one future farmer, while
other economic foundations may take helpers. New Farms tile in three-tile
cells around Mills and Town Centers; full clusters expand with another Mill.
Farm placement resumes across passes within the same 32-site budget, and
exhausting natural-resource searches does not prevent farming with the
remaining order and placement budget. The manager helps unfinished economic
foundations with the nearest unloaded idle villagers in the admitted census:
at most three builders on a House or Mill, four on a Lumber/Mining Camp. A Farm
gets one prospective farmer. Known managed builders outside a partial census
and assignments earlier in the same pass count immediately. Other workers keep
looking for resource work instead of joining a full
team. A pending Mill does not block Farms served by completed drop-offs once
the normal completed-Mill prerequisite is met. The manager prepays two normal
Mill reseeds. Every cost, exploration check, placement
rule and build/train time is the ordinary simulation rule.

Adopted farmers also build a nearby Mill for an old distant plot when a legal
site exists within six tiles of its centre. Repairs rotate between plots and
retain their searches, using at most half the remaining placement budget so
idle villagers can still reclaim or build Farms. Approaching farmers start
working once their whole body is on the crop, without waiting for an exact
reserved work point.

An accepted manual override releases a managed worker immediately and protects
its busy job until genuine idle. Previously unclaimed busy workers can still be
adopted in place. Military, foreign workers and workers outside
the circle are never adopted. Ejection/death/destruction deactivates management
and releases its tags; current ordinary jobs can finish. Autonomous resource
replacement and return targets stay within an active domain.

Automatic orders also carry the issuing domain before an idle worker has a
controller. Ordinary resource fallback must obey that territory too; a refused
assignment reserves no managed slot. Manual orders keep their ordinary rules.

The eight-search and 32-site limits still apply per admitted domain. Productive
job probes and ordinary command attempts are each capped at 64, so even a
fully unemployed crowd cannot cause unlimited job scans. Finder telemetry counts returned block lists and candidate/cache checks:
at most 25 lists per query. A bounded query returns up to eight nearest offers for reuse during the current
pass. Each search admission funds up to 1,608 candidate
checks, including retained offers reused by later workers in the same pass.
Every cache entry is counted; unused allowance is shared without admitting
more map searches. Cold map-block construction is
separate: an eight-tile block has at most 32 spatial history-range seeks, 256
retained starting identities and 64 tile probes before deduplication to 64
returned candidates. These are fixed spatial bounds, rather than a scan of the
world's history. Its storage caps are separate too: 4,096 starting identities,
2^20 depletion entries, and an 8,192-entry/32-MiB resource-block cache. Across
eight admitted domains, the unchanged search budget admits at most 64 queries
per pass. Ordinary order admission and pathfinding retain their own budgets.

The server records this phase as `steward_economy`. The synthetic 100-player,
100,000-population test checks the manager's admission and work counts. It does
not demonstrate finished RTS performance or qualify the owner's hardware.

## Presentation and licensed graphics

Selecting a completed owned Town Center shows the yellow territory of every owned
Town Center; a solid outline is an active garrisoned Steward, a dashed circle an
inactive domain. Where two active territories meet, each outline stops 0.4 tiles
short of the equal-distance border, drawn as a double line. Selected managed villagers show “Managed by
Steward”. Steward production is disabled while that Town Center has a queued or garrisoned Steward. Incoming Stewards also reserve its place on the server.
An occupied Town Center also offers “Dismiss idle villagers”: it kills idle owned
villagers in that Steward's territory that no Steward currently manages, including
stopped villagers holding a load, using normal death and population handling. Busy, managed, sheltered, foreign and
out-of-territory units are preserved. An inactive Steward cannot dismiss workers.

The original DAT's King (unit 434) supplies the pose IDs and timing. Its SLPs
1767/1771/1764/1768 are absent from the trial DRS, so the private asset import
uses the licensed DE `u_king_west_{idle,walk,death,decay}A_x1.sld` animations.
Only bounded selected frames enter the existing actor atlas. Source and derived
images remain outside Git and container images.

For a repeatable local capture, `POST /maps/reset?scenario=steward-territory`
prepares two overlapping Town Centers, 40 busy farmers and two Stewards waiting
outside (four-times clock). Garrison the first to watch it adopt every farmer,
then the second to see the territory split with a double line.
Fog is disabled, and inherited military opponents and Wolves are removed so
the waiting farmers survive this management comparison.

`POST /maps/reset?scenario=steward-farming` prepares a resource-free plot with
24 idle villagers, one Mill, a Steward entering its Town Center, and enough wood. The 20-times clock makes normal farm construction and reseeding
visible in a short capture; it does not change the manager's work budgets.

`POST /maps/reset?scenario=steward-kingdom` prepares twenty busy farmers, nearby
forest and a completed Lumber Camp, with 30,000 Food/Gold/Stone and 1,000 Wood.
Its four-times game clock preserves ordinary orders and manager budgets.
Recenter on the stationary Scout to frame the farm clusters and forest.

`POST /maps/reset?scenario=steward-stability` uses the same paid working Farms
with 1,800 Food, 1,000 Wood and a Steward waiting in a clear spot outside its
Town Center. Garrison it during the headless showcase take to observe the first
moderate productive adjustments. Farmers retain their real carried loads and
finish normal deliveries before changing jobs. It uses the same four-times
demo clock and ordinary management budgets.

`POST /maps/reset?scenario=steward-throughput` prepares forty idle villagers,
known forest, a Lumber Camp and an ungarrisoned Steward. Its ordinary 20 Hz clock
and real garrison/gather orders make the dispatch burst visible without time
acceleration. Fog is disabled for this local capture scene.

Villager training and optional reseeds/drop-off repairs retain their original
two-order support window. The larger shared budget serves productive worker
orders; it does not divert a burst of busy farmers into optional repairs.

## Coastal work

The same bounded spatial census includes owned Fishing Ships in the territory,
with a separate fleet limit of 100; ships do not consume the villager cap.
Busy fishing ships are adopted in place, and idle ships receive ordinary fish
orders. Manual moves stay manual until real idle; foreign and outside ships
are excluded. Leaving the Town Center releases the fleet, and autonomous replacement
fish and return targets remain in the active territory.

Each map domain advances one discovery block even when its land workers are
busy, retaining at most eight known, explored fish nodes. Ship-local queries
share the eight-search allowance and all cache inspections are counted. A
coastal domain with known fish builds an ordinary Dock when the territory has
none, reuses an existing foundation, and replaces a lost builder. Placement
shares the 32-site allowance. The training switch covers villagers and fishing
ships; a completed Dock trains ships only with Food priority, available Wood,
an empty queue. Inland domains without fish buy no Dock.

`POST /maps/reset?scenario=steward-fishing` prepares a synthetic flat coast,
a Steward entering a Town Center, eight villagers and one fishing ship already on
an ordinary fish job. It starts without a Dock; normal construction and
Food-priority training grow the fleet. Its one-Hz game clock runs twenty times
faster under the server ticker for local recording, with fog disabled.
