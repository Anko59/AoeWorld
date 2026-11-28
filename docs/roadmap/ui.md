# Roadmap: UI and visuals

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| A button design system: every button (game menu, diplomacy, around the minimap, bottom menu) restyled and placed consistently in a realistic medieval look | 11-28 x1 | PLANNED | Owner 11-28: "I want better looking buttons and better button placements. I want a design system for our buttons... I don't really like any of them." Style as always: realistic medieval | One component set (sizes, states, frames, icon slots) built on DE art; then migrate the game menu, diplomacy and minimap buttons, then the rest; before/after video |
| Game setup: choose the map area by placing and rescaling a rectangle on a world map, not by typing coordinates; show tiles, crossing time and mean distance to the nearest enemy for the chosen compression ratio | 11-28 x1 | PLANNED | Owner 11-28: "Inputting geo coordinates is not a human friendly way to choose a location." | World map picker with a draggable, resizable rectangle; live readout of tiles, time to cross at unit speed, mean nearest-enemy distance for the player count |
| Client crashes and cannot recover (every input then fails with "unreachable executed"); the WebGPU tab grows until Firefox runs out of memory | 11-25 x2 | PARTIAL | #1111 fixed the terrain-blending panic and added a reload message. The owner reported no further crash after the 11-25 hotfix. Recheck the merged build in play. Firefox WebGPU still grew about 90 MB/min; WASM memory and the externref table stayed flat. Switching to WebGL2 dropped Firefox from 4.1 GB to 0.46 GB. | Get an owner Firefox `about:memory` capture during growth, then reproduce the leak on the France save and add a growth check. |
| No progress bars on enemy foundations | 11-22 x1 | UNVERIFIED | #1025 has a browser regression that shows an enemy Barracks at 25% in the world and selection panel. #957 hides 0% bars and #961 keeps unfinished crossings as plain far-view markers. The owner report has not reproduced. | Recheck in play. |
| Medieval DE styling for the far-view training panel | 11-22 x1 | PLANNED | #974 introduced shared controls and #1025 landed the workflow, recorded below. | Style the panel. |
| Medieval DE styling for the Kingdom activity panel | 11-19 x1 | PARTIAL | #969 added the native frame. The panel layering and scrolling fixes are recorded below. | Recheck the remaining styling in play. |
| Units behind buildings: silhouette outline and building fires switch off and on late in a game | 10-23, 11-09, 11-14, 11-19 x4 | PARTIAL | #949 restored outlines for Imperial buildings and walls; #974 restored native DE building fires. | Recheck outlines and whether fires stay visible late in a game. |
| Farm foundation sunk while under construction; castle on sloped terrain; carcass art | 11-09 | UNVERIFIED | #751, #764; 11-14 check: finished farm flat, carcass never reproduced, foundation and slopes not caught on camera | Record a build on a hilly map |
| WebGPU black world in Firefox | 11-02, 11-03 | UNVERIFIED | Never explained in any source; 11-15 game ran WebGL2. 11-25: the owner's Firefox ran WebGPU (adapter BrowserWebGpu) with a visible world on play25; the WebGPU path's open defect is the memory leak (row above) | Test WebGPU path before the next owner build |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Far-view training: pick a destination, choose a count per click and train mixed units there. Hide upgraded-away and unavailable units | 11-22 x1 | DONE | #974 introduced shared training controls. #1025 shipped destination and count selection plus one click per available unit type. Recheck in play. |
| Road texture: roads blend with the ground; the whiter texture made it worse, roads cannot be seen | 11-20, 11-22 x2 | DONE | #1025 draws the DE road sheet with hard edges and disables ground blending. Recheck in play. |
| Hide progress bars of buildings at 0%; show foundations in the far view without progress bars | 11-20 x1 | DONE | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units): mass placement makes the bars overwhelming; merged in #957 and #961 |
| Bottom menu design system / DSL so it reacts uniformly | 11-09 x1 | DONE | Reviewed 71 before-and-after selection pairs on the DE pack. Selection cards, queues, managers and shortcuts use shared rendering and dispatch. General and Steward share the original-art slider.; merged in #963 |
| Unseeded farm and Dark Age Town Center art still wrong (reported again; compare with AoE2 DE references online) | 11-03, 11-09, 11-19e x3 | DONE | #974 draws bare furrows until seeding completes; recheck in play. #1036 compared the Dark Age Town Center with the DE composite; the owner confirmed 11-24 that the art was correct. |
| Bottom menu does not use the full width at the owner's resolution; Steward and Vassal controls cramped in the right corner; Vassal "Focus" unclear | 11-22 x1 | DONE | Merged in #1025: Steward and Vassal controls get a wide middle column at 1600 px and above; the slider is named Development priority with an explanation. Recheck in play. |
| Holding Shift on a military building's train buttons shows a king-like unit in the bar and shifts every button right (caused misclicks that deleted buildings) | 11-22 x1 | DONE | Merged in #1025: Delete and Cancel own fixed cells, Shift shows a x5 overlay without moving any button; the king icon was not reproduced; reproduced on the owner save: the Castle's first train button is the Vassal (King portrait) and Shift batched five; managers never batch now, Delete has a fixed far cell; not re-checked in play |
| Kingdom activity panel hidden behind other far-view menus | 11-19 x1 | DONE | #969 raised the panel above the far-view menus. Recheck in play. |
| Kingdom activity panel must show all content with no vertical or horizontal scroll | 11-22 x1 | DONE | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units); merged in #1020: no 6-row limit, rows flow into columns, no scrollbar |
| Far view shows every building, no cap: enemy buildings vanished and new own buildings still missing | 11-20, 11-22 x2 | DONE | #957 prioritised own sites within the 2,048-site cap. #1020 removed the cap, sent all sites in 128-tile groups and culled by camera. On the Anatolia 4 save, 12 → 34 settlements were drawn at unchanged frame time. Recheck in play. |
| Far view: holding the click to drag a selection that starts on a unit zooms onto the unit | 11-20 x1 | DONE | #969 (far-view drags select units); owner confirmed done 11-23 |
| First near-to-far view transition after loading a game is broken, later ones are smooth | 11-20 x1 | DONE | #974 (the first zoom after loading a save follows the wheel); owner confirmed done 11-23 |
| Far view: wood resource markers stay after the wood is cut | 11-20 x1 | DONE | #969 (depleted wood stops appearing); owner confirmed done 11-23. Not the same as saves forgetting depleted resources ([gameplay](gameplay.md)) |
| Cliff texture repeats visibly: keep our cliff shapes, replace only the texture | 11-20 x1 | DONE | #974 (cliff walls use continuous DE rock samples); owner confirmed done 11-23 |
| Tech icons and unit icons for new content | 11-15 | DONE | #969 (the DE pack resolves the full unit and research icon roster) |
| Large army orders draw one move/attack-move line per unit (hundreds of lines) | 11-19e x1 | DONE | #931 (one shared marker per waypoint, orders end cleanly); owner confirmed done 11-23 |
| A group that has arrived and stopped still shows its lines and flag to the last destination | 11-19e x1 | DONE | #931; owner confirmed done 11-23 |
| Tech tooltips for ALL technologies (not only the new content): what each does, in research buildings and in the far view | 11-19e x1, 11-20 x1 | DONE | #969 (both research panels explain all 105 technologies); owner confirmed done 11-23 |
| Far view: training-units panel uncomfortable | 11-09 x1 | SUPERSEDED 11-22 | by the far-view training workflow above and the open styling row |
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
