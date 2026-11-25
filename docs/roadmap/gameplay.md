# Roadmap: gameplay and content

Open items first, most urgent on top. Statuses and rules: [roadmap](../roadmap.md).
Dates are 2026 (MM-DD); "xN" is how many separate times the owner asked.

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Gold piles contain more gold | 11-25 x1 | PLANNED | Owner 11-25, part of the long-game balance (gold too scarce) | Set the amount with the balance measurements on long AI games |
| Regrowing forests: a forest patch cut but not removed regrows from its remaining trees and slowly retakes the land it once covered; a patch cut down completely is gone for good | 11-25 x1 | PLANNED | Owner 11-25; management offices manage their land's forests sustainably | Growth rules and rates, then office logic |
| Wildlife and fish reproduce and regain their numbers after being partly hunted; complete removal is definitive; wildlife populations grow larger than today to allow it | 11-25 x1 | PLANNED | Owner 11-25: resources must be managed, and management offices know how to optimise that on their land | Population rules per species, then office logic |
| Save and reload forgets which trees the player knows were chopped | 11-22 x1 | DONE (#1036) | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units). #924 saved building sightings only | #1036: per-player resource memory saved and restored with the fog memory (snapshot version 29); a rebuilt owner save needs a new save to carry it |
| Too many shallows; buildings can be placed on shallows | 11-22 x1 | DONE (#1036) | #1036: Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | #1036: placement refuses Ford tiles for buildings (walls, roads and gates may cross); fords every 64 tiles, streams over 1.5 tiles only at fords: -74% ford tiles on Finland 30 km, -66% on France. Existing packages regenerate fords at load (terrain is computed from stored river geometry) |
| "The order queue is full" and "that order is too large" when placing many buildings (the owner places ~20 at once at this scale) | 11-20 x1 | DONE | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units); merged in #957 | Merge #955 (`night/mass-build`: place many buildings at once); then replay the 20-building placement on the Anatolia 4 save |
| Units near enemies do not engage; many watch from afar | 11-06, 11-09, 11-15, 11-19, 11-19e, 11-20 x6 | PARTIAL | Fixes: #719, #822, #840 (attack-move fights), #828 (Generals lead with ordinary orders). Quality metrics melee exchange 2.38-2.73 after fix. Owner 11-15 min 115: "many units stand close to enemies but don't engage". 11-19: "takes too long to engage", up to 2/3 of units stand still in a typical fight; 11-19 evening: engage late and stop fighting; live attack approaches waited p50 344 ticks for a route (5,793 units waiting, 18 players at the search cap) - route queue first; 11-20: Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units): groups of idle units hardly engage enemies passing by; engagement rates still low overall. 11-21: an idle formation member that engages makes its idle mates look from where it stands (20-militia block, enemies walking past 3 tiles off: 12 to 18+ engage; scoreboard +0.07%); one extra near-route search slot for fights changed neither hits nor approach waits (killed). 11-23, #1026: attack-move look limits and the idle acquisition ration dropped ([caps](caps.md)); R1 war replay first hits 2,585 to 4,607, hits +39%, kills 357 to 545, at +6.3% instr/tick | Drop the attack-move look and idle-acquisition caps ([caps](caps.md), owner 11-23); scale queue Q3 (engagement as assignment, [scale](scale.md)); sightings shared inside an idle formation (11-21, `scale/idle-engage`); attack approaches still wait a median ~290 ticks for a route on the scoreboard; melee that cannot engage still closes in; six General soldiers in melee never hit (undiagnosed) |
| Units auto-target walls and roads; DE attacks them only when nothing else is in sight or when ordered | 11-19 x1 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); the first attempt was closed unmerged; automatic targeting prefers visible units and buildings over walls, gates and roads; merged in #958 | Recheck in the next owner game |
| Rock tiles are walkable | 11-19 x1 | IN FLIGHT #926 | Owner playtest 11-19 (Anatolia 2); DE rocks are large. #926 keeps map recipe 4, so existing maps and saves load; rock footprints are baked into chunk navigation (late_game -0.24% instructions vs dev) | Release #926 once the owner no longer needs the Anatolia 4 save, or land it without a recipe bump |
| Civilisations and unique units | 10-24 (deferred), 10-30 | PLANNED | Owner 11-14 asked for one complete generic civ first; none started | After generic content lands: pick one civ, brief a track |
| Bigger maps and map size independent of player count | 10-26, 10-30 | PARTIAL | #787 (Mekong, Asia rivers), #810 (natural fair starts); owner 11-15 played a new Anatolia map (960 km, 240:1) | Scale work in [scale](scale.md) decides how big maps can go |
| Farm seeding, farms vs dirt blend | 11-03, 11-09 | UNVERIFIED | #686 (seeding farms), #764 (road look); "farms sometimes merge with dirt" never re-checked | Look at farms on dirt in a real match, film if wrong |
| Attack-move toward an enemy base answers "destination blocked" by enemy buildings; attack-move must path through and destroy them | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); checked 11-21: #931 only changed attack-move queue display, no merged PR changes route targets | Treat enemy buildings on an attack-move route as targets, not obstacles |
| Shift-queued production (5 units) takes a long time to register | 11-19e x1 | PARTIAL | Owner playtest 11-19 evening (Anatolia 2 v4, build play18). #909 merged: Shift-click and Shift+Q issue up to five affordable train orders and fill the queue slots. Registration latency itself was not measured | Measure the order path on a big save; batch train messages only if the delay remains |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| "Too many orders at once" when stopping a whole army or sending 30 cavalry to scout | 11-19, 11-19e x2 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); 11-19 evening: still refused for large armies. #925 merged: Stop and Auto-scout go as one list order (one admission per list, finished over later ticks). Not covered: move, attack and attack-move for large selections; merged in #1020 and #1028: 256 orders/s, burst 1,024, 2,048 pending per connection, 16,384 server-wide, every ready order handled each tick; large selections are sent in several messages of up to 4,096 units; #1028: gather, patrol, garrison, hunt and General walk go as list orders of up to 4,096 units |
| Navy has no attack animations | 11-19e x1 | DONE | #916 (ship attack effects, Fire Ship flames, DE attack clocks). Owner playtest 11-19 evening |
| Guard Tower and Keep researched but towers stay Watch Towers | 11-19e x1 | DONE | #938 (Guard Tower and Keep apply to existing and new towers, with their own art and stats). Owner playtest 11-19 evening |
| Foundations not yet worked on are visible and targetable by other players; in AoE they are local-only until a villager starts building | 11-19e x1 | DONE | #938 (a planned foundation reserves tiles for its owner and becomes public and blocking when work begins). Owner playtest 11-19 evening |
| Save and load: scouted enemy buildings in fog vanish | 11-19 x1 | DONE | #924 (per-player last sightings preserved across saves, hidden damage and destruction in a round-trip test). Owner playtest 11-19 (Anatolia 2) |
| Garrison button icon wrong; add Back to Work and the Town Bell, as in DE | 11-19 x1 | DONE | #912 (native DE garrison art, a 25-tile Town Bell, Back to Work). Owner playtest 11-19 (Anatolia 2) |
| Hand Cannoneer attack animation too fast; Trebuchet unlike DE | 11-15 x1 | DONE | #916 (DAT firing duration for the Hand Cannoneer, stationary Trebuchet pack and unpack, native boulder); covered by the owner 11-20 content verdict |
| New content integrated and playable end to end: Cavalry Archer, Scorpion, Hand Cannoneer, Bombard, full post-overhaul navy, Husbandry, Chemistry, fish traps | 11-14 x1, broken 11-15, 11-19 x2 | DONE | Owner verdict 11-20: "new content is also very good". Merged: #916 (roster plays end to end, shared across selection and training, DE art, 59 trainable kinds), #872 (navy and late land units), #855 (34 techs, Monk conversion), #857 (trade, Wonder). The 11-15 and 11-19 "broken" reports are superseded by that verdict. |
| Auto-scout turns off after loading a save | 11-19 | DONE | Active scouts resume exploration from the saved position. The snapshot round-trip verifies active and idle scouts (#918) |
| Shift-click and Shift+Q train five units | 11-19 | DONE | Uses ordinary train orders and fills up to the affordable queue slots (#909). |
| Castle Age, garrisons, bridges (directional, any direction), navy base, river fish, formations, placement ghosts | 10-24 | DONE | #570 |
| Hidden attack bonuses, DE elevation (hill) rules, the General army leader | 10-29 x1 | DONE | #630, #631, #633 |
| Imperial Age with units, techs, art; no placeholders | 10-30, 11-03 | DONE | #653, #673; owner confirmed 11-11 |
| Roads (Feudal, faster travel), Steward and defensive Vassal split | 10-29, 11-03 | DONE | #693, #685, #688 |
| Market trading, bulk trade from far view, caravans | 10-24 | DONE | #538, #712, #857 |
| Population space from buildings with no maximum | 10-31, corrected 11-03 | DONE | #647 (removed cap), #665 (restored housing) |
| Hunting, wolves, deer flee, TC builders, bridge snap, old lumber camps | 11-09 | DONE | #762 |
| Wider castle and TC arrow volleys; kingdom activity panel | 11-09 | DONE | #805 |
| Rally points, Ctrl queue (x3), idle markers, 15-unit queues, front spawns | 10-23, 10-28 | DONE | #607, #571 |
| Spread over trees, build over units, no lock inside buildings | 10-26, 10-29 | DONE | #638 |
| Delete key, builders never block their foundation | 10-26 | DONE | #655, #651 |
| Fishing, fog of war, Feudal Age, Dark Age buildings | 10-20 | DONE | #520, #519, #524, #506 |
| Stone walls and gates, correct orientation | 10-29, 11-05 | DONE | #642, #777 |
| Pause and save/resume from the menu | 11-06 | DONE | #718 |
| Shift-queued attack-move, owner-controlled Generals, fog-off ownership | 11-09 | DONE | #800 |
| Starting trees survive fog (bots at 0 wood) | 11-14 | DONE | #837 |
| Natural and fair player starts | 11-09 | DONE | #810 |
| River fords every 40 tiles (now 64, streams over 1.5 tiles only at fords) | 11-12 | DONE | #801; 11-24 #1036 |
| War Galley DE bolt, ship auto-scout | 11-09, 11-06 | DONE | #767, #722 |
| Single lone unit attack-move keeps intent and fights | 11-14 | DONE | #822, #840 |
