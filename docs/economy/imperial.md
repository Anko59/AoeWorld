# Imperial Age

The generic Imperial roster has no civilisation bonuses, unique units or unique
technologies. Research Imperial Age at an owned, completed Town Center with an
empty queue: 1,000 food, 800 gold and 190 game seconds, Castle Age, and two
distinct completed kinds from Castle, University, Monastery and Siege Workshop.
Towers, Docks and duplicate buildings do not count. Earlier unlocks remain.
Cancelling refunds the whole price. The player's age, and every owned
building's age, becomes Imperial when the research ends; the HUD shows
"IV  Imperial Age" and `Requires Imperial Age.` for refused orders.

## Authoritative data

Numbers come from the owner's licensed DE **VER 8.9** DAT (source BLAKE3
`ee111552cc11cfbf535a5e4bc28adb7137ce4b2eee68fd2faf3bac96f9235c9e`): research
costs, times and prerequisites from the technology table (research 103 is
Imperial Age) and unit hit points, attack, armour, reload, range and speed from
the unit records. Technology ids and effects were read with a local scan of the
inflated DAT, never committed. Only generic effects are implemented; Rust owns
them in `research/imperial.rs`, the unit table and `combat/classes/profiles.rs`.

| Technology (DAT id) | Building | Food / wood / gold | Seconds | Needs |
| --- | --- | --- | --- | --- |
| Imperial Age (103) | Town Center | 1000 / 0 / 800 | 190 | Castle Age, 2 buildings |
| Two-Handed Swordsman (217) | Barracks | 200 / 0 / 100 | 45 | Long Swordsman |
| Champion (264) | Barracks | 650 / 0 / 350 | 70 | Two-Handed Swordsman |
| Halberdier (429) | Barracks | 300 / 0 / 600 | 50 | Pikeman |
| Arbalester (237) | Archery Range | 450 / 0 / 350 | 50 | Crossbowman |
| Cavalier (209) | Stable | 300 / 0 / 300 | 80 | Knights |
| Paladin (265) | Stable | 1300 / 0 / 750 | 170 | Cavalier |
| Hussar (428) | Stable | 500 / 0 / 600 | 50 | Light Cavalry |
| Capped Ram (96) | Siege Workshop | 300 / 0 / 0 | 50 | |
| Siege Ram (255) | Siege Workshop | 1000 / 0 / 0 | 75 | Capped Ram |
| Onager (257) | Siege Workshop | 800 / 0 / 500 | 75 | |
| Blast Furnace (75) | Blacksmith | 275 / 0 / 225 | 100 | Iron Casting |
| Plate Mail Armor (77) | Blacksmith | 300 / 0 / 150 | 70 | Chain Mail Armor |
| Plate Barding Armor (80) | Blacksmith | 350 / 0 / 200 | 75 | Chain Barding Armor |
| Bracer (201) | Blacksmith | 300 / 0 / 200 | 40 | Bodkin Arrow |
| Ring Archer Armor (219) | Blacksmith | 250 / 0 / 250 | 70 | Leather Archer Armor |
| Crop Rotation (12) | Mill | 250 / 250 / 0 | 70 | Heavy Plow |
| Two-Man Saw (221) | Lumber Camp | 300 / 200 / 0 | 100 | Bow Saw |
| Stone Shaft Mining (279) | Mining Camp | 175 / 75 / 0 | 75 | Stone Mining |

Every row except Imperial Age itself requires Imperial Age.

| Unit / DAT id | HP | Attack | Melee / pierce armour | Notes |
| --- | --- | --- | --- | --- |
| Two-Handed Swordsman / 473 | 65 | 12 | 1 / 1 | +4 vs standard buildings |
| Champion / 567 | 70 | 14 | 1 / 1 | |
| Halberdier / 359 | 60 | 6 | 0 / 0 | +32 cavalry, +17 ships, +26 fishing ships |
| Arbalester / 492 | 40 | 6 pierce | 0 / 0 | range 5 |
| Cavalier / 283 | 120 | 12 | 2 / 2 | 1.8 s reload |
| Paladin / 569 | 160 | 14 | 2 / 3 | 1.9 s reload |
| Hussar / 441 | 75 | 7 | 0 / 2 | +12 vs Monks |
| Capped Ram / 422 | 200 | 3, +160 buildings, +50 siege | -2 / 190 | |
| Siege Ram / 548 | 270 | 4, +200 buildings, +65 siege | -1 / 195 | |
| Onager / 550 | 60 | 55, +45 buildings | 0 / 8 | range 3-8, LOS 10 |

Training costs and times equal the base unit of each line (the DAT keeps them),
so upgrading changes strength, not price. The line mechanics are unchanged from
Castle Age: an upgrade replaces the line's trainable identity, converts living
units keeping damage taken (hit points rise by the difference), and converts queued
units keeping their fraction of training progress. Training the base kind after an
upgrade queues the newest identity. Ram and artillery identities share the Ram and
Mangonel rules (stance, retaliation, targeting, splash) through `UnitKind::is_ram`
and `is_artillery`.

Blacksmith tier three stacks on the earlier tiers: Blast Furnace +2 attack for
infantry and cavalry; Plate Mail +1 melee and +2 pierce armour for infantry,
Plate Barding the same for cavalry and Ring Archer Armor the same for archers;
Bracer +1 attack, range and line of sight for archers (and Castle/Tower/Town
Center arrows). Crop Rotation adds 175 food per new Farm, Two-Man Saw +20% wood
gathering and Stone Shaft Mining +15% stone gathering, using the same exact
fixed-point rates as the earlier tiers.

Buildings keep Castle Age hit points, line of sight, armour and art in Imperial
Age: the DAT's Imperial architecture sets are not imported, and the render tag
space (256 values) is full, so Imperial buildings and the new units reuse the
Castle Age buildings and the line's base art. A DE overlay for Imperial art needs a
wider tag first.

## Wire and AI

`Tech` grows to 53 entries (the 64-bit research set has room for 11 more) and
`UnitKind` to 34. Appended discriminators keep earlier wire values. Postcard tests
pin `ImperialAge` (34), `StoneShaftMining` (52), `Onager` (33),
`RejectedRequiresImperialAge` (38) and the age/technology consistency rules. The
browser content asset stores a unit-kind training mask as two words. The Python and
Rust SDKs mirror the enums.

The example AI is described in the [AI SDK guide](../ai-sdk.md): it raises a University and a
Siege Workshop, saves gold, advances at 40 villagers and a 24-soldier army, then trains
and researches the newest identities. The remaining Imperial Age content (Treadmill
Crane, Architecture, Chemistry and Bombard Cannons, Heavy Scorpions, Siege Onagers,
Imperial Skirmishers, camels and Husbandry) is not part of this slice.

## Showcase

`POST /maps/reset?scenario=imperial-army` prepares a Castle Age base with a
University, Siege Workshop, Barracks, Archery Range, Stable and Blacksmith, a
Castle Age army and six distant Militia. It runs the ordinary game on a ten-times
clock (the demo scenarios' convention); prices, research, training and orders
are unchanged. Open `/?play=1`, research Imperial Age at the Town Center, then
the unit lines.
