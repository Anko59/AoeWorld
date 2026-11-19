# Roadmap: gameplay and content

Open items first, most urgent on top. Statuses and rules: [roadmap](../roadmap.md).
Dates are 2026 (MM-DD); "xN" is how many separate times the owner asked.

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| "The order queue is full" and "that order is too large" when placing many buildings (the owner places ~20 at once at this scale) | 11-20 x1 | PLANNED | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units) | Raise or split the limits for multi-building placement; one order per batch |
| New content integrated and playable end to end: Cavalry Archer, Scorpion, Hand Cannoneer, Bombard, full post-overhaul navy, Husbandry, Chemistry, fish traps | 11-14 x1, broken 11-15, 11-19 x2 | IN FLIGHT `night/content-roster` | #855 added 34 techs and Monk conversion, #857 added trade and the Wonder, and #872 added the navy and late land units. The owner described the content as "broken end to end" at minute 150 and "unfinished and broken" on 11-19 (Anatolia 2, 4.0 ticks/s at 61.5k units on the 10 Hz build). #916 shares the roster across selection and training, restores DE art, and covers orders for all 59 trainable kinds and work at a Fish Trap in Playwright. The normal-clock DE showcase covers 35 entries. | Review and ship #916, then verify the new content in a real save |
| Units near enemies do not engage; many watch from afar | 11-06, 11-09, 11-15, 11-19, 11-19e, 11-20 x6 | PARTIAL | Fixes: #719, #822, #840 (attack-move fights), #828 (Generals lead with ordinary orders). Quality metrics melee exchange 2.38-2.73 after fix. Owner 11-15 min 115: "many units stand close to enemies but don't engage". 11-19: "takes too long to engage", up to 2/3 of units stand still in a typical fight; 11-19 evening: engage late and stop fighting; live attack approaches waited p50 344 ticks for a route (5,793 units waiting, 18 players at the search cap) - route queue first; 11-20: Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units): groups of idle units hardly engage enemies passing by; engagement rates still low overall | Scale queue Q3 (engagement as assignment, [scale](scale.md)); then share sightings across a formation; melee that cannot engage still closes in; six General soldiers in melee never hit (undiagnosed) |
| Hand Cannoneer attack animation too fast; Trebuchet unlike DE | 11-15 x1 | IN FLIGHT `night/content-roster` | #916 tests the Hand Cannoneer's DAT firing duration and the Trebuchet's pack and unpack state and native boulder. Close footage shows both weapons in action. | Ship #916 |
| Units auto-target walls and roads; DE attacks them only when nothing else is in sight or when ordered | 11-19 x1 | IN FLIGHT `night/no-wall-target-v2` | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); #923 rebased: automatic targeting prefers visible units and buildings over walls, gates and roads | Exclude walls, gates and roads from automatic target acquisition unless no other target is in sight |
| Garrison button icon wrong; add Back to Work and the Town Bell, as in DE | 11-19 x1 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) #912 | Native DE garrison art, a 25-tile Town Bell, and Back to Work |
| "Too many orders at once" when stopping a whole army or sending 30 cavalry to scout | 11-19, 11-19e x2 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); a player-facing refusal of a large selection; 11-19 evening: still refused when sending large armies after #925 (batched Stop and Auto-scout only); cover move, attack and attack-move orders; #925 batches Stop and Auto-scout for large selections | Split large selections over ticks server-side instead of refusing |
| Save and load: scouted enemy buildings in fog vanish | 11-19 x1 | DONE | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) #924 | Save per-player last sightings and cover hidden damage and destruction in a round-trip test |
| Rock tiles are walkable | 11-19 x1 | IN FLIGHT `night/rock-footprints` | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); DE rocks are large | Block the full measured DE rock footprint in movement and map views |
| Civilisations and unique units | 10-24 (deferred), 10-30 | PLANNED | Owner 11-14 asked for one complete generic civ first; none started | After generic content lands: pick one civ, brief a track |
| Bigger maps and map size independent of player count | 10-26, 10-30 | PARTIAL | #787 (Mekong, Asia rivers), #810 (natural fair starts); owner 11-15 played a new Anatolia map (960 km, 240:1) | Scale work in [scale](scale.md) decides how big maps can go |
| Farm seeding, farms vs dirt blend | 11-03, 11-09 | UNVERIFIED | #686 (seeding farms), #764 (road look); "farms sometimes merge with dirt" never re-checked | Look at farms on dirt in a real match, film if wrong |
| Attack-move toward an enemy base answers "destination blocked" by enemy buildings; attack-move must path through and destroy them | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | Treat enemy buildings on an attack-move route as targets, not obstacles |
| Shift-queued production (5 units) takes a long time to register | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | Measure the order path; batch train messages |
| Foundations not yet worked on are visible and targetable by other players; in AoE they are local-only until a villager starts building | 11-19e x1 | IN FLIGHT | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | `night/towers-foundations-tooltips`: server keeps planned sites private and passable until work starts |
| Guard Tower and Keep researched but towers stay Watch Towers | 11-19e x1 | IN FLIGHT | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); match data showed 22 WatchTower after both techs | `night/towers-foundations-tooltips`: existing and future towers use the researched kind and DE art |
| Navy has no attack animations | 11-19e x1 | IN FLIGHT | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); #916 (not in that build) adds ship attacks and Fire Ship flames | Ship #916 |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
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
| River fords every 40 tiles | 11-12 | DONE | #801 |
| War Galley DE bolt, ship auto-scout | 11-09, 11-06 | DONE | #767, #722 |
| Single lone unit attack-move keeps intent and fights | 11-14 | DONE | #822, #840 |
