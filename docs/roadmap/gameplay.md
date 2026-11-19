# Roadmap: gameplay and content

Open items first, most urgent on top. Statuses and rules: [roadmap](../roadmap.md).
Dates are 2026 (MM-DD); "xN" is how many separate times the owner asked.

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| New content integrated and playable end to end: Cavalry Archer, Scorpion, Hand Cannoneer, Bombard, full post-overhaul navy, Husbandry, Chemistry, fish traps | 11-14 x1, broken 11-15, 11-19 x2 | PARTIAL | Merged: #855 (34 techs, Monk conversion), #857 (Trade Cart, Trade Cog, Wonder), #872 (naval and Castle/Imperial land units). Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s): "unfinished and broken", second report; no agent has played each new unit end to end. Owner at game min 150: new content "broken end to end" | Derive the HUD "military" list and unit names from game data (`web/interface/hud.js` lines 83 and 130 omit new kinds, so names and icons shift); add tech icons; add an e2e that trains and orders every unit kind; play each new unit on a real save and film it |
| Units near enemies do not engage; many watch from afar | 11-06, 11-09, 11-15, 11-19 x4 | PARTIAL | Fixes: #719, #822, #840 (attack-move fights), #828 (Generals lead with ordinary orders). Quality metrics melee exchange 2.38-2.73 after fix. Owner 11-15 min 115: "many units stand close to enemies but don't engage". 11-19: "takes too long to engage", up to 2/3 of units stand still in a typical fight | Scale queue Q3 (engagement as assignment, [scale](scale.md)); then share sightings across a formation; melee that cannot engage still closes in; six General soldiers in melee never hit (undiagnosed) |
| Hand Cannoneer attack animation too fast; Trebuchet unlike DE | 11-15 x1 | PLANNED | Owner report 11-15; no PR | Fix with the content integration above |
| Units auto-target walls and roads; DE attacks them only when nothing else is in sight or when ordered | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Exclude walls, gates and roads from automatic target acquisition unless no other target is in sight |
| Shift-click trains 5 at once, as in DE | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Batch train on shift in the bottom menu and hotkeys |
| Garrison button icon wrong; add Back to Work and the Town Bell, as in DE | 11-19 x1 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Native DE garrison art, a 25-tile Town Bell, and Back to Work |
| "Too many orders at once" when stopping a whole army or sending 30 cavalry to scout | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); a player-facing refusal of a large selection | Split large selections over ticks server-side instead of refusing |
| Save and load: scouted enemy buildings in fog vanish | 11-19 x1 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Save per-player last sightings and cover hidden damage and destruction in a round-trip test |
| Save and load: auto-scout turns off for every unit | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Restore active scouts and verify save round-trip |
| Rock tiles are walkable | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); DE rocks are large | Make rock footprints block walking, matching the DE art size |
| Civilisations and unique units | 10-24 (deferred), 10-30 | PLANNED | Owner 11-14 asked for one complete generic civ first; none started | After generic content lands: pick one civ, brief a track |
| Bigger maps and map size independent of player count | 10-26, 10-30 | PARTIAL | #787 (Mekong, Asia rivers), #810 (natural fair starts); owner 11-15 played a new Anatolia map (960 km, 240:1) | Scale work in [scale](scale.md) decides how big maps can go |
| Farm seeding, farms vs dirt blend | 11-03, 11-09 | UNVERIFIED | #686 (seeding farms), #764 (road look); "farms sometimes merge with dirt" never re-checked | Look at farms on dirt in a real match, film if wrong |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Shift-click and Shift+Q train five units | 11-19 | DONE | Uses ordinary train orders and fills up to the affordable queue slots. |
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
| River fords every 40 tiles | 11-12 | DONE | #801 |
| War Galley DE bolt, ship auto-scout | 11-09, 11-06 | DONE | #767, #722 |
| Single lone unit attack-move keeps intent and fights | 11-14 | DONE | #822, #840 |
