# Resource ground and garrisons

Placement and navigation both consult the live resource overlay. The immutable
generator still describes a tree, mine or bush after exhaustion; its object
list is not a placement veto. Partial depletion blocks ground until the last
resource is harvested. Prepared navigation keeps that same overlay contract.

## Shelter rules

Town Centers hold 15 villagers or foot soldiers (including foot archers).
Watch Towers hold 5. Castles hold 20 units of any current kind. Only completed,
owned buildings accept entry. Orders reserve capacity while units walk to the
building, and Stop or a replacement job cancels the reservation. Entry occurs
at the footprint or its adjacent ring, without teleporting approaching units.

Occupants leave the map, targeting, movement, wildlife and vision indexes.
They retain identity, population, hit points and attack cooldown. Town Center
entry deposits carried resources; other shelters retain the load with its unit. Town Centers and towers heal 1 HP every 10 seconds; Castles heal
1 HP every 5 seconds, capped at the unit's current maximum. Loom and
Man-at-Arms also upgrade units inside shelters.

Ungarrison all or click an occupant icon to eject one. Exits are deterministic
free tiles in four local rings. Blocked exits retain occupants. Destruction
opens the footprint and ejects occupants, stacking survivors there if every
other exit is occupied; rare wholly blocked terrain exits retain a
bounded pending release and retry once per second. Units and inventory are
never discarded for a blocked exit.

## Arrows and original data

The original unit records are Town Center 109/71, Watch Tower 79 and Castle 82.
The local numeric survey confirms the healing rates and Castle's 4x4 footprint,
4800 HP, 650 stone, 11 pierce attack and 8/11 armour. No source imagery is
embedded in code or committed.

Empty Town Centers fire zero arrows, towers one, and Castles five. Villagers
contribute 5 attack per 2 seconds; Archers and Skirmishers contribute their
researched pierce damage divided by reload time. Sum this firepower before
dividing by one building arrow's firepower and flooring. Total volley limits
are 10, 5 and 21, respectively. Fletching affects the unit/building firepower
and villagers' effective garrison attack. Other infantry add no arrows.
The [Siege Engineers knowledge base](https://siegeengineers.github.io/Aoe2Sheet/knowledge_base.html)
describes the summed-DPS calculation. Existing indexed target queries, fog
visibility, projectile flight, armour and death handling resolve the volley.
Entry/ejection cycles preserve the building's reload timer.

Right-click an eligible healthy shelter or use Garrison to enter a damaged
one; ordinary right-click on a damaged building still repairs it. The building
panel shows owner-visible occupants, their HP and ejection controls. The
current bounded protocol carries occupant records only in their building;
Rust and Python SDKs send the same Garrison/Ungarrison orders as the browser.

Castles require Castle Age. Defensive buildings automatically prefer units,
then armed buildings and other structures. Explicit in-range attack orders
override automatic selection without resetting the reload timer. Empty Town
Centers cannot shoot. Town Bell automation is not implemented.
