# Roadmap: UI and visuals

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Far view shows every building, no cap: enemy buildings vanished and new own buildings still missing | 11-20, 11-22 x2 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units). #957 kept a 2,048-site cap and sorted own buildings first; with tens of thousands of own buildings the cap now drops every enemy site. Owner: "a stupid bandaid ... No cap" | Send all sites (chunked or per far-view tile group, already grouped on screen) with no fixed cap; prove with the 86.7k-building save |
| No progress bars on enemy foundations | 11-22 x1 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | Bars only for the player's own foundations above 0% |
| Holding Shift on a military building's train buttons shows a king-like unit in the bar and shifts every button right (caused misclicks that deleted buildings) | 11-22 x1 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | Reproduce Shift-batch training in the bottom menu presenter (#963); no layout shift on modifier keys |
| Kingdom activity panel must show all content with no vertical or horizontal scroll | 11-22 x1 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | Fit or summarise rows; no scrollbars |
| Bottom menu does not use the full width at the owner's resolution; Steward and Vassal controls cramped in the right corner; Vassal "Focus" unclear | 11-22 x1 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | Lay out at the owner's resolution; label Focus (Fortify vs Roads development priority) in plain words |
| Far-view training panel: hide upgraded-away and untrainable units; medieval DE style; train a large mixed army in few clicks: pick a location, pick a count per click, then click each unit to train that count and send it there | 11-22 x1 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units). The UI batch 2 rework (#974) did not improve it and dropped the DE style | Redesign to the owner's flow |
| Road texture: roads blend with the ground; the whiter texture made it worse, roads cannot be seen | 11-20, 11-22 x2 | PLANNED | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units) | Distinct DE road texture with clear edges; compare stills |
| Far view: holding the click to drag a selection that starts on a unit zooms onto the unit | 11-20 x1 | PLANNED | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units) | Zoom-to-unit only on a click released without drag |
| First near-to-far view transition after loading a game is broken, later ones are smooth | 11-20 x1 | PLANNED | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units) | Reproduce on a loaded save; likely first-use cache build |
| Far view: wood resource markers stay after the wood is cut | 11-20 x1 | PLANNED | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units) | Refresh far-view resource markers on depletion |
| Far view: some recently built buildings do not appear (possibly a limit) | 11-20 x1 | PARTIAL | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units) (86.7k buildings in the save); #957: the far view kept the first 2,048 sites in id order, so a new building behind a large foreign base was cut; your own buildings now come first. Not checked on the 86.7k-building save | Check on the owner save in the next build |
| Hide progress bars of buildings at 0%; show foundations in the far view without progress bars | 11-20 x1 | DONE | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units): mass placement makes the bars overwhelming; merged in #957 and #961 | Merge #955 (`night/mass-build`: no 0% bars, foundations and newest buildings in the far view) |
| Cliff texture repeats visibly: keep our cliff shapes, replace only the texture | 11-20 x1 | PLANNED | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units). Owner: "that's not what the definitive edition does. I like your cliff more than the definitive edition ones. I just don't like the texture." | Replace the cliff texture with one that does not visibly tile; leave the cliff shapes unchanged |
| Kingdom activity panel not in the medieval DE style; hidden behind other menus in the far view | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); panel from #805 | Restyle on DE art, raise it above the far-view menus |
| Units behind buildings: silhouette outline and building fires switch off and on late in a game | 10-23, 11-09, 11-14, 11-19 x4 | PARTIAL | On Anatolia 3, the #765 effect counter stopped when occluders dropped to zero even though eligible units remained. #949 merged: outline coverage restored for Imperial buildings and stone walls. Building fires switching off and on late in a game: no merged fix found 11-21 | Keep the fire report open; recheck outlines and fires in the next owner game |
| Bottom menu design system / DSL so it reacts uniformly | 11-09 x1 | DONE | Reviewed 71 before-and-after selection pairs on the DE pack. Selection cards, queues, managers and shortcuts use shared rendering and dispatch. General and Steward share the original-art slider.; merged in #963 | Review the walkthrough and the remaining growth across the full patch |
| Far view: training-units panel uncomfortable | 11-09 x1 | PLANNED | #712 cockpit redesign; panel not revisited | Refine with the owner's example |
| Farm foundation sunk while under construction; castle on sloped terrain; carcass art | 11-09 | UNVERIFIED | #751, #764; 11-14 check: finished farm flat, carcass never reproduced, foundation and slopes not caught on camera | Record a build on a hilly map |
| WebGPU black world in Firefox | 11-02, 11-03 | UNVERIFIED | Never explained in any source; 11-15 game ran WebGL2 | Test WebGPU path before the next owner build |
| Tech icons and unit icons for new content | 11-15 | PLANNED | Missing for #855 techs; #916 and #872 added DE icons for the roster (units and commands). Tech icons not rechecked 11-21 | Check tech icons in the research panels with the tooltip work |
| Large army orders draw one move/attack-move line per unit (hundreds of lines) | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); checked 11-21: #931 gave each group order one shared marker but not one grouped line; #953 (large army orders as one command) is open | One grouped line per order from the group's centre to each waypoint |
| A group that has arrived and stopped still shows its lines and flag to the last destination | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); owner worries it costs CPU | Clear the queue display on arrival; check the order really ended server-side |
| Tech tooltips for ALL technologies (not only the new content): what each does, in research buildings and in the far view | 11-19e x1, 11-20 x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18). Owner 11-20: "I want tooltips for all techs, not just the new one". Checked 11-21: #938 (branch `towers-foundations-tooltips`) shipped tower and foundation fixes without tooltips; none on dev | Separate change from tower and foundation fixes. Structure the mechanics behind generic effect tags, then render effects, cost and time in both research panels. |
| Unseeded farm and Dark Age Town Center art still wrong (reported again; compare with AoE2 DE references online) | 11-03, 11-09, 11-19e x3 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | Compare with online DE reference images side by side before changing anything |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Far-view resources: fish drawn one by one fill the sea; no wood indicator | 11-19e x1 | DONE | #929 (fish merged into fishing grounds, explored woodland marked) |
| Far view: roads drawn over buildings and farms vanish at the last zoom level before the far view | 11-19 x1 | DONE | #949 (roads under the settlement layer, farms kept across zoom steps). Recheck in the next owner game |
| Far view: show each group's dominant unit as its bottom-menu portrait and distinguish villagers, fishing ships and warships | 11-19, 11-19e x2 | DONE | #929 (DE selection-card portraits with owner rims and counts for far-view groups). Owner playtests on Anatolia 2 |
| Attack-move queue controls and waypoint colours | 11-19 x1 | DONE | #931 (dashed gold move lines and pennants, solid red attack-move lines and brass-rimmed shields, one shared marker per waypoint, Stop clears the queue). Owner playtest follow-up to #800 |
| Occlusion around buildings, collapse art, ruins and large DE rocks | 11-19 x1 | DONE | #949 (footprint depth for buildings, collapse art, ruins and large rocks). Owner playtest 11-19; recheck in the next owner game |
| Garrison button uses the native DE doorway icon | 11-19 | DONE | Imported action 069, with native icons for Back to Work and the Town Bell (#912) |
| Selected unit does not show whether auto-scout is on | 11-19 | DONE | The auto-scout button shows a solid gold outline for active scouts and a dashed one for mixed selections (#918) |
| Multi-building training queues overlap | 11-19 | DONE | Each selected producer has its own readable queue row in the bottom menu (#909). |
| Dark Age Town Center side (11-09 visuals item 1) | 11-09 | DONE | #661 Town Center slice origin; does not reproduce (11-14, flat ground, default camera) |
| DE art for every unit, building, terrain object; no flicker | 10-26, 10-31 x3 | DONE | #644 |
| Far-zoom cockpit: stats, build, jump keys, units here, layers, charts, bulk trade, readable style | 10-30, 11-02, 11-03, 11-05 | DONE | #671, #679, #684, #687, #692, #712 |
| Bottom menu on DE art without scrolling (x2) | 11-05 | DONE | #706, #714 |
| Grid under buildings; age-up progress bar | 11-10 | DONE | #763 |
| Kingdom activity panel (training, research) | 11-09 | DONE | #805 |
| No yellow dotted circles; F1 labels; castle corner; bridge feet; far view follows age | 11-09 | DONE | #751, #764 |
| TC queue and garrison visible; far view hides construction | 11-09 | DONE | #751 |
| Intro timing, random art, one monotonic loading bar | 11-06 | DONE | #717, #720 |
| Menu colours, portrait clicks, flames, click priority, one farmer per farm | 11-03 | DONE | #667 |
| Burning buildings, siege bodies, projectiles in sync | 10-28 | DONE | #611, #618, #640 |
| Native carry art, seeding farms | 11-03 | DONE | #686 |
| Client FPS telemetry reported | 11-09 | DONE | #754 |
