# Generic technologies (content track B)

The generic tree gains the 34 non-naval, non-regional technologies of the
Barracks, Archery Range, Stable, University, Monastery, Castle and Market.
Cost, research time, Age, building and prerequisite come from the owner's
licensed DE DAT (research records read with a local scan, never committed; ids
below are DAT research ids). Effect text comes from the DE help strings of the
same install. Rust owns every number: `research/generic.rs` (table),
`research/generic_effects.rs` (effects), `combat/conversion/mod.rs` (Monks).

`researched` is now a 128-bit mask (`TechMask`); a test pins that every
`Tech` fits. Replay hashes and saves change with it.

| Technology (DAT id) | Building / Age | Effect as implemented |
| --- | --- | --- |
| Squires (215) | Barracks / Castle | infantry speed +10% |
| Arson (602) | Barracks / Feudal | infantry +2 attack vs buildings |
| Gambesons (875) | Barracks / Castle | militia line +1 pierce armour |
| Thumb Ring (437) | Archery Range / Castle | archers and skirmishers reload 15% faster; accuracy is already 100% (no accuracy dice exist) |
| Parthian Tactics (436) | Archery Range / Imperial | mounted archers +1/+2 armour, +2 vs spear line (no mounted archers yet: the hook is `UnitKind::is_mounted_archer`) |
| Bloodlines (435) | Stable / Feudal | mounted units +20 HP, existing and new |
| Husbandry (39) | Stable / Castle | mounted units speed +10% |
| Architecture (51) | University / Imperial, needs Masonry | buildings (not walls, gates, Farms) +10% HP and +1/+1 armour; the +3 building armour is not modelled |
| Siege Engineers (377) | University / Imperial | artillery +1 range, siege +20% of its bonus vs buildings; Demolition Ships are not covered |
| Heated Shot (380) | University / Castle | Watch Towers +125% attack, Castles and Docks +4 vs ships |
| Arrowslits (608) | University / Imperial | Watch Tower +1, Guard Tower +2, Keep +3 attack |
| Fortified Wall (194) | University / Castle | Stone Wall 1080 -> 3000 HP, armour 12/12; gates keep their stats (no DAT record) |
| Guard Tower (140), Keep (63) | University / Castle, Imperial | Watch Towers upgrade in place: HP 850 -> 1500 -> 2250, attack 5 -> 7 -> 8, armour 2/8 -> 3/9 |
| Treadmill Crane (54, "Stone cutting") | University / Castle | construction 20% faster |
| Devotion (46), Faith (45) | Monastery / Castle, Imperial | enemy conversion succeeds 85% / 50% of the time |
| Fervor (252), Sanctity (231) | Monastery / Castle | Monk speed +15%, Monk HP +15 |
| Herbal Medicine (441) | Monastery / Castle | garrison healing six times as fast |
| Theocracy (438) | Monastery / Imperial | when Monks convert a target together only the converter rests |
| Illumination (233) | Monastery / Imperial | conversion rest halved (62 s -> 31 s) |
| Block Printing (230) | Monastery / Imperial | Monk range +3 |
| Atonement (319) | Monastery / Castle | Monks may convert enemy Monks |
| Redemption (316) | Monastery / Castle | Monks may convert siege and buildings (not Town Centers, Castles, Monasteries, Farms, walls, gates) |
| Heresy (439) | Monastery / Castle | units of its owner die when converted |
| Conscription (315) | Castle / Imperial | Barracks, Archery Ranges, Stables and Castles train 33% faster |
| Hoardings (379) | Castle / Imperial | Castles +1000 HP |
| Sappers (321) | Castle / Imperial | villagers +15 attack vs buildings, +3 vs rams |
| Spies/Treason (408) | Castle / Imperial | research only: the reveal command is not implemented |
| Caravan (48) | Market / Castle | `trade_speed_percent` = 120 for the trade track's units |
| Guilds (15) | Market / Imperial | market commission 30% -> 15% |
| Coinage (23), Banking (17) | Market / Castle, Imperial | `tribute_fee_permyriad` 30% -> 20% -> 0; Tribute is not a command yet |

## Monk conversion (new mechanic)

A Monk given an attack order on an enemy tries to convert it whenever its
weapon is ready; success is rolled from a deterministic hash of tick, Monk and
target. A success replaces the target by a new unit of the same kind, position
and hit points under the Monk's owner (a new id; population moves with it); the
Monk rests 62 seconds. Not modelled: automatic conversion in aggressive stance,
conversion delay by distance, the adjacency rule for buildings and rams, and
conversion of Town Centers, Castles and Wonders. Generals, Vassals and Stewards
cannot be converted. A converted building is rebuilt under the new owner on the
same footprint with its hit points; its garrison is ejected and queues are lost.
