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

Empty Town Centers fire zero arrows. An occupied Town Center fires its DAT
base of one plus one per occupant, capped at eleven. Castles fire five plus
one per occupant, capped at twenty-one. This deliberately replaces the old
summed-DPS rule for these two buildings to make sheltered armies visibly add
arrows (#745). All admitted occupants count, including infantry.

Watch Towers retain one base arrow, plus summed villager/foot-archer DPS
divided by one tower arrow's DPS, floored and capped at five.
Villagers contribute 5 attack per 2 seconds, plus researched arrow equipment;
foot archers contribute their researched pierce damage divided by reload.

The VER 8.9 survey now exposes projectile counts, spawning area and accuracy
dispersion. Castle/TC launch positions span the DAT's half-tile square;
secondary aim points alternate across that same area for spread impacts.
This deterministic fan is a presentation/gameplay choice, not random accuracy
(their DAT dispersion is zero). Offsets are calculated once at launch; normal
projectile flight, hit-distance checks, armour and death handling still apply.
No extra targeting queries or per-tick fan calculations are added.
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
