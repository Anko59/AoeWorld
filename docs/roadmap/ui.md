# Roadmap: UI and visuals

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Occlusion: units hidden by buildings they stand in front of and the opposite; destroyed-building animation, ruins and the large DE rocks always draw on top | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Sort by footprint base, not sprite anchor; check ruins, rubble and rocks with `make compare` |
| Attack-move queue controls and waypoint colours | 11-19 x1 | IN FLIGHT `night/attack-move-queue` | Owner playtest follow-up to #800. Gold pennants and dashed move lines. Brass-rimmed red shields and solid attack-move lines. Paths start at the selected units. Right-click orders, left-click selects or clears selection, Shift release finishes placement, Escape exits placement, and Stop clears orders. Each group order has one shared marker. Attack-move retains later waypoints through combat. | | Merge #931 |
| Kingdom activity panel not in the medieval DE style; hidden behind other menus in the far view | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s); panel from #805 | Restyle on DE art, raise it above the far-view menus |
| Far view: show each group's dominant unit as its bottom-menu portrait and distinguish villagers, fishing ships and warships | 11-19, 11-19e x2 | IN FLIGHT | Owner playtests on Anatolia 2. PR #929 now uses the DE selection-card portraits with owner rims and counts. | Owner review and ship |
| Far view: roads drawn over buildings; farms vanish at the last zoom level before the far view | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s) | Layer order and farm LOD |
| Units behind buildings: silhouette outline and building fires switch off and on late in a game | 10-23, 11-09, 11-14, 11-19 x4 | PARTIAL | #611 picking over buildings, #786 related case, #765 telemetry says why a silhouette is missing. Not reproduced in a short session (11-14) | Read telemetry from a long session; open until seen |
| Bottom menu design system / DSL so it reacts uniformly | 11-09 x1 | PLANNED | #706 and #714 reworked it (DE art, no scrolling) but no standard layer | Brief a design-system pass |
| Far view: training-units panel uncomfortable | 11-09 x1 | PLANNED | #712 cockpit redesign; panel not revisited | Refine with the owner's example |
| Farm foundation sunk while under construction; castle on sloped terrain; carcass art | 11-09 | UNVERIFIED | #751, #764; 11-14 check: finished farm flat, carcass never reproduced, foundation and slopes not caught on camera | Record a build on a hilly map |
| WebGPU black world in Firefox | 11-02, 11-03 | UNVERIFIED | Never explained in any source; 11-15 game ran WebGL2 | Test WebGPU path before the next owner build |
| Tech icons and unit icons for new content | 11-15 | PLANNED | Missing for #855 techs | With content integration |
| Large army orders draw one move/attack-move line per unit (hundreds of lines) | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | One grouped line per order from the group's centre to each waypoint |
| A group that has arrived and stopped still shows its lines and flag to the last destination | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units); owner worries it costs CPU | Clear the queue display on arrival; check the order really ended server-side |
| Tech tooltips: what each technology does, in research buildings and in the far view | 11-19e x1 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | Tooltips from the tech data (effects, cost, time) |
| Far-view resources: fish drawn one by one fill the sea; no wood indicator | 11-19e x1 | IN FLIGHT | Owner evening playtest (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units). PR #929 sums known fish nodes into a handful of fishing grounds and marks explored woodland. | Owner review and ship |
| Unseeded farm and Dark Age Town Center art still wrong (reported again; compare with AoE2 DE references online) | 11-03, 11-09, 11-19e x3 | PLANNED | Owner playtest 11-19 evening (Anatolia 2 v4, build play18, 3.6 ticks/s at 61k units) | Compare with online DE reference images side by side before changing anything |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
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
