# Vassal

The Vassal is an original AoeWorld unit using the licensed King graphic.
A completed Castle in Castle Age trains one for 1,000 food and 1,000 gold
in 60 game seconds, using one population slot. A queued Vassal reserves the
Castle too. Cancellation refunds the full cost; death permits a replacement.

Its home is the Castle that trained it. A Vassal can shelter only there.
Orders outside a circular radius of 100 tiles from the Castle centre are
refused with a specific message, preserving the current order. Movement and
soft separation also enforce the bound, including detours. Transports and
formations cannot carry it outside. If the Castle is destroyed, a surviving
Vassal retains that home's location and cannot activate another Castle.

The default population ceiling is 1,000 per player. Houses provide five slots
and completed Castles twenty until that ceiling; queued units reserve slots.
[World capacities](../scale/capacity.md) records the matching storage bounds
and the distinction between capacity and performance qualification.

## Deterministic economy

Sheltering the Vassal at home activates its circular domain. The Rust simulation
visits up to eight active domains every 20 ticks (one second at 20 Hz), rotating
in entity order. Each domain scans at most 1,024 spatial-bucket entries and 2,048
of its owner's buildings; a retained cursor progresses through crowded buckets.
A partial census never authorizes training or a new foundation. Each admitted
domain may attempt two ordinary orders, eight bounded resource searches and 32
placement checks. Site searches resume across passes. The world maxima per pass
are therefore 8,192 unit visits, 16,384 building visits, 16 orders, 64 resource
searches and 256 site checks. Ordinary pathfinding retains its shared budgets.

### Exact rules (what is adopted, when TCs train, why it stops)

**Territory.** A Vassal's territory is the 100-tile circle around its Castle,
cut by its sibling Vassals: every point belongs to the nearest *active* Castle
of the same player (integer squared distance, ties to the lower Vassal ID), so
no villager, resource node or building ever has two Vassals. A Castle is active
while its Vassal is garrisoned in it; an inactive Castle claims nothing and its
neighbour takes its area at the next pass. A building belongs to the Vassal
whose territory holds its centre, and must also lie wholly in the circle.

**Adoption.** Each pass a Vassal scans its territory and adopts, in entity
order, up to 100 of the owner's villagers: busy ones (gathering, farming,
building) are taken over in place and keep working; idle ones are given work
by the job pass. Villagers under a manual order (plain move, garrison order)
are left alone until idle again, and an accepted manual order releases a worker
at once. Soldiers, foreign units and units outside the territory are never
adopted. The old rule adopted only idle villagers, so a Castle built among
busy workers showed a small "managed" count whatever the crowd; that was the
reported "20 managed" confusion.

**Training.** Each complete Town Center in the territory with an empty queue
trains one villager per pass (two orders per pass per Vassal) when the player
has 50 food and the population cap allows, until residents (every living
villager in the territory, sheltered ones included) plus queued villagers reach
100. It stops, and the HUD says why, when: the census is incomplete (more than
1,024 units in the circle), residents+queued is 100, no Town Center lies in the
territory, population (cap) is reached, or food is short. Population is the
usual reason in a large crowd: only an idle worker used to build the needed
House, so a fully busy Vassal never built one. Now, when the cap is within two
slots, the first unloaded managed worker builds a House.

**Stalls.** Every pass the Vassal checks its managed workers. One whose task is
Moving but whose position has not changed for 8 seconds is stopped and
re-tasked, avoiding the node it was heading to (after a second consecutive
stall, avoiding that resource kind). The client treats a villager reported as
Moving that is neither moving nor planning as idle for the idle-villager count.

The HUD line for a selected Castle reads "n/100 villagers managed · r in
territory · reason", with the counts coming from the server (the old text
counted only units inside the camera subscription).

Budgets and limits:

The 100-villager limit counts living residents, including manual workers and
sheltered villagers, plus queued villagers in every domain Town Center. Training
uses complete TCs with their spawn perimeter and rally inside the domain; it
respects the player's food and all reserved population slots. Deaths permit
replacements. Each domain can directly manage at most 100 workers.

Jobs use the kingdom's shared stocks and actual deliveries, with default
food:wood:gold:stone weights of 45:30:15:10. The lowest weighted projected
reserve gets priority: current stock plus deliveries in the recent minute of
game time. Comparisons use integer cross-products. Local worker counts do not
stand in for income. Each player retains twelve five-second delivery buckets;
ordinary resource returns and carried loads deposited on garrison entry count
only the amount actually credited. Purchases, spending and refunds do not
create harvested income. Every bucket's epoch and amounts participate in the
canonical hash, including income earned before a Vassal becomes active.

Idle workers and unloaded managed gatherers respond to those kingdom needs.
Carried loads finish their ordinary delivery first, and a refused replacement
keeps the current productive job. Training and essential housing retain their
normal admission before productive reassignment; optional maintenance follows.
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
foundations, prepays two normal Mill reseeds, and adds Houses when two or fewer
housing slots remain. Every cost, exploration check, placement
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

The original eight-search, two-order and 32-site limits still apply per admitted
domain. Finder telemetry counts returned block lists and candidate/cache checks:
at most 25 lists and 1,608 checks per query. Cold map-block construction is
separate: an eight-tile block has at most 32 spatial history-range seeks, 256
retained starting identities and 64 tile probes before deduplication to 64
returned candidates. These are fixed spatial bounds, rather than a scan of the
world's history. Its storage caps are separate too: 4,096 starting identities,
2^20 depletion entries, and an 8,192-entry/32-MiB resource-block cache. Across
eight admitted domains, the unchanged search budget admits at most 64 queries
per pass. Ordinary order admission and pathfinding retain their own budgets.

The server records this phase as `vassal_economy`. The synthetic 100-player,
100,000-population test checks the manager's admission and work counts. It does
not demonstrate finished RTS performance or qualify the owner's hardware.

## Presentation and licensed graphics

Selecting a completed owned Castle shows the yellow territory of every owned
Castle; a solid outline is an active garrisoned Vassal, a dashed circle an
inactive domain. Where two active territories meet, each outline stops 0.4 tiles
short of the equal-distance border, drawn as a double line. Selected managed villagers show “Managed by
Vassal”. Training is disabled while that Castle has a queued or living Vassal.

The original DAT's King (unit 434) supplies the pose IDs and timing. Its SLPs
1767/1771/1764/1768 are absent from the trial DRS, so the private asset import
uses the licensed DE `u_king_west_{idle,walk,death,decay}A_x1.sld` animations.
Only bounded selected frames enter the existing actor atlas. Source and derived
images remain outside Git and container images.

For a repeatable local capture, `POST /maps/reset?scenario=vassal-territory`
prepares two overlapping Castles, 40 busy farmers and two Vassals waiting
outside (four-times clock). Garrison the first to watch it adopt every farmer,
then the second to see the territory split with a double line.

`POST /maps/reset?scenario=vassal-farming` prepares a resource-free plot with
24 idle villagers, one Mill, a Vassal entering its Castle, and enough wood and
housing. The 20-times clock makes normal farm construction and reseeding
visible in a short capture; it does not change the manager's work budgets.

`POST /maps/reset?scenario=vassal-kingdom` prepares twenty busy farmers, nearby
forest and a completed Lumber Camp, with 30,000 Food/Gold/Stone and 1,000 Wood.
Its four-times game clock preserves ordinary orders and manager budgets.
Recenter on the stationary Scout to frame the farm clusters and forest.
