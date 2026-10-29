# Hidden combat bonuses

Research checked 2026-10-29. This feature uses the DE bonus rules associated
with the existing roster and naval tech tree (before the naval replacement
technologies). It does not import original AoK bonus values. There is one
runtime ruleset, with no selectable historical profiles.

## Sources and interpretation

- [DE extracted class data, revision 0adfc656](https://github.com/SiegeEngineers/aoe2techtree/blob/0adfc65635368a15cc7e2f3b3f54eb8e39c88523/data/data.json)
  supplies numeric `Attacks` and `Armours`, including the separate fishing-ship
  class. This snapshot implements [DE update 128442](https://www.ageofempires.com/news/age-of-empires-ii-definitive-edition-update-128442/).
- [Attack bonus](https://ageofempires.fandom.com/wiki/Attack_bonus) and
  [AoE2 armour classes](https://ageofempires.fandom.com/wiki/Armor_class_(Age_of_Empires_II))
  describe matching classes and independent subtraction. Direct Fandom HTTP
  requests returned a challenge page; indexed unit pages were consulted.
- [Villager](https://ageofempires.fandom.com/wiki/Villager_(Age_of_Empires_II)),
  [Spearman](https://ageofempires.fandom.com/wiki/Spearman_(Age_of_Empires_II)),
  [Scout Cavalry](https://ageofempires.fandom.com/wiki/Scout_Cavalry_(Age_of_Empires_II)),
  [Battering Ram](https://ageofempires.fandom.com/wiki/Battering_Ram_(Age_of_Empires_II)),
  and [Mangonel](https://ageofempires.fandom.com/wiki/Mangonel_(Age_of_Empires_II))
  cross-check identities, bonuses, and blast behavior.
- [DE update 125283](https://ageofempires.fandom.com/wiki/Update_125283)
  raised the Battering Ram building bonus to 150.

The latest DE naval rebalance replaces War Galley research with new warship
technologies and changes both base stats and classes. Those replacement systems
are outside this feature. Existing base HP, attack, melee/pierce armour, reload,
and movement tables remain authoritative; their provenance and qualification
limits are in [military](../military.md) and [fishing](../fishing.md).

For each attack component whose class the target actually has, compute
`max(0, attack[class] - armour[class])`; sum these components, then enforce a
minimum total damage of one. An absent class is immune to that component; it
does not mean zero armour. Base melee (4) and pierce (3) use the same rule.
Negative melee armour (notably Ram -3 and Dark House -2) increases that component.
An attack component of zero is distinct from an absent component.

## Every current unit

All units have base melee/pierce armour from `UNIT_COMBAT`; the table lists
additional armour classes and their armour values (zero unless indicated).
Only attack classes with targets in our roster are listed. A dash means none.

| Unit (DE id) | Extra armour classes | Bonus attacks |
| --- | --- | --- |
| Villager (83) | — | All Buildings +3; Stone Defense +6 |
| Scout Cavalry (448) | Cavalry | Monks +6 |
| Militia (74) | Infantry | — |
| Fishing Ship (13) | Fishing Ships | — |
| Archer (4) | Archers | Spearmen +3 |
| Skirmisher (7) | Archers; Skirmishers | Archers +3; Spearmen +3 |
| Spearman (93) | Infantry; Spearmen | Cavalry +15; Ships +9; Fishing Ships +9; Standard Buildings +1 |
| Man-at-Arms (75) | Infantry | Standard Buildings +2 |
| Long Swordsman (77) | Infantry | Standard Buildings +3 |
| Pikeman (358) | Infantry; Spearmen | Cavalry +22; Ships +16; Fishing Ships +16; Standard Buildings +1 |
| Crossbowman (24) | Archers | Spearmen +3 |
| Elite Skirmisher (6) | Archers; Skirmishers | Archers +4; Spearmen +4 |
| Knight (38) | Cavalry | — |
| Light Cavalry (546) | Cavalry | Monks +10 |
| Monk (125) | Monks | — |
| Mangonel (280) | Siege Weapons | All Buildings +35; Siege Weapons +12 |
| Battering Ram (1258) | Siege Weapons; Rams | All Buildings +150; Siege Weapons +40 |
| Galley (539) | Ships | Ships +8; Fishing Ships +8; All Buildings +6; Rams +3 |
| Fire Ship (529) | Ships (6) | Ships +3; Fishing Ships +3; All Buildings +2; additional melee +1 |
| Demolition Ship (527) | Ships (3) | All Buildings +220 |
| Transport Ship (545) | Ships | — |
| War Galley (21) | Ships | Ships +9; Fishing Ships +9; All Buildings +7; Rams +4 |
| Vassal (custom) | — | — (cannot attack) |

Villagers have no generic infantry, archer, or cavalry class. Monks have their
own class. Fishing Ships use their own class instead of also taking the Ships
component twice. Fire Ship combines pierce base damage with a separate melee
component; armour reduces each independently. Ship-class armour reduces only
the matching ship component, never base pierce or building damage.

Class ids: Infantry 1, Pierce 3, Melee 4, Cavalry 8, All Buildings 11,
Stone Defense 13, Archers 15, Ships 16, Rams 17, Siege Weapons 20,
Standard Buildings 21, Walls/Gates 22, Monks 25, Castles 26, Spearmen 27,
Fishing Ships 34, Skirmishers 38. Unused classes without any current attacker
or target need no runtime slot (including armour-ignoring class 31).

## Every current building

All have base melee/pierce armour from `age_armour`, All Buildings (11), and
Standard Buildings (21). The two building classes are distinct. Standard
Buildings armour is always zero. Extra classes and nonzero class armour:

| BuildingKind | Extra classes / armour | Bonus attacks |
| --- | --- | --- |
| TownCenter | — | All Buildings +5; Ships +5; Fishing Ships +5 (all arrows) |
| House | — | — |
| LumberCamp | — | — |
| MiningCamp | — | — |
| Mill | — | — |
| Farm | — | — |
| Barracks | — | — |
| PalisadeWall | Walls/Gates | — |
| Outpost | — | — |
| Dock | — | — (no defensive dock system) |
| ArcheryRange | — | — |
| Stable | — | — |
| Blacksmith | — | — |
| Market | — | — |
| WatchTower | Stone Defense | Spearmen +2; Ships +7; Fishing Ships +7 |
| PalisadeGate | Walls/Gates | — |
| Castle | Castles; All Buildings armour 8 | Spearmen +2 |
| Monastery | — | — |
| University | — | — |
| SiegeWorkshop | — | — |
| Bridge (custom) | Walls/Gates (palisade analogue) | — |

Castles do **not** have the Stone Defense class; a Villager's +6 against
Stone Defense applies to Watch Towers, not Castles. Masonry adds +1 melee,
+1 pierce, and +3 All Buildings armour to eligible buildings. It does not
reduce Standard Buildings bonuses. Farms and palisades/gates are excluded.

Unit upgrades replace the kind, so Pikeman, Elite Skirmisher, Crossbowman,
Long Swordsman, Light Cavalry, and War Galley immediately select their new row
for existing and subsequently trained units. Blacksmith upgrades modify only
base attack/armour. The tech tree has no Arson, Sappers, or Siege Engineers.

## Implementation and limits

Fixed class profiles build immutable pair tables at compile time. Unit hits
index one row; building hits select ordinary/Masonry rows. No per-hit allocation
or hashing is needed for class damage. Projectiles retain their launch bonus,
including building bonuses, so removing the shooter does not erase them.
Mangonel area hits select each victim's classes separately.

No camels, elephants, eagles, cavalry archers, gunpowder, unique units, stone
walls, or civilization-specific bonus resistance exist in the current roster.
Vassal and Bridge are project-specific choices, not claims about DE units.
Independent licensed DE extraction and dedicated-hardware qualification remain
external work. Instruction workloads are synthetic performance evidence.
