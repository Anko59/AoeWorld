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

The 100-villager limit counts living residents, including manual workers and
sheltered villagers, plus queued villagers in every domain Town Center. Training
uses complete TCs with their spawn perimeter and rally inside the domain; it
respects the player's food and all reserved population slots. Deaths permit
replacements. Each domain can directly manage at most 100 workers.

Idle workers use food:wood:gold:stone shares of 45:30:15:10, adjusted toward low
stockpiles, preserving carried loads. Local known resources within ten tiles
are preferred. Distant drop-offs prompt normal Lumber/Mining Camp or Mill
construction; food also uses free Farms, builds new ones, and prepays two normal
Mill reseeds. The manager helps unfinished economic foundations and adds Houses
when two or fewer housing slots remain. Every cost, exploration check, placement
rule and build/train time is the ordinary simulation rule.

An accepted manual order releases the worker immediately; busy manual workers
are left alone until idle again. Military, foreign workers and workers outside
the circle are never adopted. Ejection/death/destruction deactivates management
and releases its tags; current ordinary jobs can finish. Autonomous resource
replacement and return targets stay within an active domain.

The server records this phase as `vassal_economy`. The synthetic 100-player,
100,000-population test checks the manager's admission and work counts. It does
not demonstrate finished RTS performance or qualify the owner's hardware.

## Presentation and licensed graphics

Selecting a completed owned Castle shows its 100-tile domain circle; the solid
circle and status indicate an active garrisoned Vassal, and a dashed circle
indicates an inactive domain. Selected managed villagers show “Managed by
Vassal”. Training is disabled while that Castle has a queued or living Vassal.

The original DAT's King (unit 434) supplies the pose IDs and timing. Its SLPs
1767/1771/1764/1768 are absent from the trial DRS, so the private asset import
uses the licensed DE `u_king_west_{idle,walk,death,decay}A_x1.sld` animations.
Only bounded selected frames enter the existing actor atlas. Source and derived
images remain outside Git and container images.
