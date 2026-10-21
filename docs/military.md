# Feudal military

The owner asked: "if you have time you can continue with the fog of war,
fishing and the feudal age". Military uses the existing economy, research,
visibility, training, movement and death paths.

## Sources

`make assets-dat-table-de` reads the licensed
`resources/_common/dat/empires2_x2_p1.dat`, **VER 8.9**, compressed BLAKE3
`ee111552cc11cfbf535a5e4bc28adb7137ce4b2eee68fd2faf3bac96f9235c9e`.
Ignored `local-assets/dat-table/feudal-de.json` records offsets, costs, combat
classes, speeds, timings, graphics, projectiles and raw effects. Original data
stays outside Git and Docker images. The focused decoder follows the public
[Genie unit layout](https://github.com/SiegeEngineers/genieutils-py/blob/master/src/genieutils/unit.py).
The independently extracted [DE data](https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json)
and installed `CivTechTrees/BRITONS.json` cross-check identities and sites.
The licensed version wins when values differ.

## Units

Costs are food/wood/gold, armour is melee/pierce, times are game seconds,
speed is tiles/second. Melee DAT range is zero; simulation melee retains its
existing one-tile centre-to-edge reach.

| Unit (DE id) | Cost F/W/G | HP | Attack | Armour | Range | Reload | Speed | LOS | Train |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Archer (4) | 0/25/45 | 30 | 4 pierce | 0/0 | 0–4 | 2 | 0.96 | 6 | 35 |
| Skirmisher (7) | 25/35/0 | 30 | 2 pierce | 0/3 | 1–4 | 3 | 0.96 | 6 | 26 |
| Spearman (93) | 35/25/0 | 45 | 3 melee | 0/0 | melee | 3 | 1.00 | 4 | 22 |
| Man-at-Arms (75) | 50/0/20 | 45 | 6 melee | 0/1 | melee | 2 | 0.96 | 4 | 21 |
| Scout Cavalry (448), Dark | 80/0/0 | 45 | 3 melee | 0/2 | melee | 2 | 1.20 | 4 | unavailable |
| Scout Cavalry (448), Feudal | 80/0/0 | 45 | 5 melee | 0/2 | melee | 2 | 1.55 | 6 | 30 |

This install prices Militia (74) at **50 food/20 gold**: it keeps 40 HP,
4 attack, 0/1 armour, 0.90 speed and 21-second training. This supersedes the
earlier 60-food value. Skirmisher's 26-second training and Man-at-Arms'
cost/speed are deliberate local measurements.

Ranges train Archers/Skirmishers; Barracks train Militia and Feudal Spearmen.
Man-at-Arms research (222/effect 211) costs 100 food/40 gold, takes 40 seconds,
and upgrades living Militia and later line training. Both HP values gain five,
preserving damage; dead Militia retain their death art. Stables train the same
Scout kind used at start. Effect **16** adds two attack, two LOS and 0.35 speed.
[Forgotten Empires' scouting guide](https://www.forgottenempires.net/strategy/age-of-empires-ii-strategy-center/scouting)
documents the starting scout, Nomad exception, Eagle Scout differences and
automatic age upgrades. AoeWorld retains one starting Scout Cavalry for its
generic civilization. Population-only diagnostics keep configured movement.

## Damage and ranged hits

The DE class subset is melee (4), pierce (3), cavalry (8), archer (15) and
spear-line (27). Spearmen add 15 against cavalry; Skirmishers add three against
archers (including Skirmishers) and Spearmen; Archers add three against Spearmen.
Class components are reduced independently, then total damage is at least one.
Other DE attack/armour classes and civilization bonuses remain unimplemented.

Ranged combat uses **delayed hits with visible native projectiles**. DE
projectiles **363** (Archer), **365** (Skirmisher) and **504** (tower) all move
at **7.0 tiles/s** here. Integer travel is ceil(distance × tick rate / speed),
at least one tick. Units and Watch Towers share launch/impact/damage/death code.
Shots retain the aim point sampled at launch and hit the original target at impact;
accuracy, ballistic evasion, collision, friendly fire and terrain occlusion
are absent. Ungarrisoned Town Centers do not fire; garrison commands are absent.

Server attack commands require visible targets; ranged attacks also require
continued visibility to launch shots. A launched shot may finish after LOS is
lost or its shooter dies. Both
server projection and client rendering require the shot's current tile to be
visible. Wire positions describe only the visible remainder; unseen endpoints
are redacted. Storage is capped at 4,096 live shots; ticks scan that bounded
set and active attackers, never all unit pairs. Shots enter replay hashes.

Out-of-range attacks search the attacker's reachable tiles for a firing position
inside its researched maximum range and outside its minimum range. Melee uses
the same search with one-tile reach. Water, occupied building footprints and
uncrossable edges block movement, while targets on those tiles can still be hit
from shore or an adjacent tile. Canonical integer search bounds expansions,
visited tiles and retained frontier entries to 8,192 each. Rejection says no
reachable attack position was found within that bounded search; it does not
prove disconnection beyond the bound. Chasing retains the one-second repath
cadence. The server reports the existing unreachable-route acknowledgement,
so the client explains that no reachable route was found. `make perf-timing`
includes 16 approach orders against a blocked Town Center footprint; hosted
timings remain informational.

## Blacksmith

All effects apply to existing and future units/buildings and use ordinary
Feudal gating, unique research, payment, busy queues, cancellation and refunds.

| Tech (DE id/effect) | Food/gold | Seconds | Effect |
| --- | --- | --- | --- |
| Forging (67/67) | 150/0 | 50 | infantry/cavalry +1 melee attack |
| Scale Mail Armor (74/74) | 100/0 | 40 | infantry +1 melee/+1 pierce armour |
| Scale Barding Armor (81/81) | 150/0 | 45 | cavalry +1 melee/+1 pierce armour |
| Fletching (199/192) | 100/50 | 30 | archers, Skirmishers, towers +1 attack/range/LOS; TC +1 attack/LOS |
| Padded Archer Armor (211/202) | 100/0 | 40 | archers/Skirmishers +1 melee/+1 pierce armour |

Fletching's TC commands modify attributes 9, 1 and 23, **not 12 (range)**:
TC range stays six. Selection attack, armour and range use researched rules.

## Native sprites and budgets

`make assets-dat-table` separately decodes trial **VER 5.7** sprite metadata.
Units use `unit_visuals`, signed hotspots, five stored rows/eight mirrored
facings, native frame durations, player colour and die/decay timing.

| Unit | Stand | Walk | Attack | Die | Decay |
| --- | --- | --- | --- | --- | --- |
| Archer | 8 | 12 | 2 | 5 | 9 |
| Skirmisher | 1650 | 1654 | 1644 | 1647 | 1651 |
| Spearman | 873 | 877 | 867 | 870 | 874 |
| Man-at-Arms | 1044 | 1048 | 1038 | 1041 | 1045 |
| Scout Cavalry | 2085 | 2089 | 2079 | 2082 | 2086 |

Projectile graphics 3378/3399 select SLP **3799/3820**, five drawn directional
cells each. Long actor animations sample alternate native cells while keeping
the native loop duration. Death is one-shot. Source prefixes remain checked
for truncation/duplicates. The combined synthetic native/DE selection pins
**4,628** frames under the unchanged **5,120** cap, four 2048² pages/64 MiB.
Real-pack packing/visual evidence is separate from this synthetic budget test.
The local WebGPU capture packs **4,459** real-art frames into the same four
pages; its metadata and screenshots stay under `.cache/military-shots/`.

## Wire, gates and demo

Unit kinds 4–7 and technologies 8–13 append. Building state appends
authoritative `gate_open`; fog memory retains the last observation. Server
variant **10** carries bounded full projectile replacements at a subscription
revision/tick, including empty sets on subscribe/resync. Rust goldens and
browser `wire/*` pin one current protocol; no legacy decoder is retained.

Gate art never scans resident units. A bounded cache recomputes visible gates
only when authoritative open/construction inputs change, using prepared terrain
height for culling. Visibility uses the renderer's selected sprite frames,
native hotspots and projected bounds at the current zoom, including open gates
and construction stages. Regressions cover absent unseen holding units,
unchanged gates, zoom-three overlap and gates wholly outside the viewport.

`POST /maps/reset?scenario=feudal-military` prepares a rich Feudal village with
Range, Barracks, Stable, Blacksmith, housing and two enemy targets. Its five-Hz
game clock runs under the ordinary 20-Hz ticker (4×). Commands stay ordinary.
The server scenario writes `.cache/military/scenario.log`; the added village
browser test trains an Archer/Spearman, researches Fletching and attacks.
This is synthetic functional evidence, not finished RTS performance or
dedicated-hardware qualification.
