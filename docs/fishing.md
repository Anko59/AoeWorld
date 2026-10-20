# Dark Age fishing

Fishing uses Food stockpiles and the same transactional depletion overlay as
other resources. Shore fish sit in water next to walkable land; stationary
deep fish sit beyond the shallow and medium rim. Placement uses a separate
coordinate hash, shares the shoreline memo, and never scans the world. Rivers,
fords, marsh shallows and water strips less than twenty-three tiles across cannot
receive shore fish. Deep fish require the existing deep-water classification.
Package generation recipe 2 is the only current recipe; regenerate local
packages rather than keeping a reader for recipe 1. Current-output map pins
are regenerated when the recipe changes.

The Dock requires a free 3×3 footprint containing both land and navigable water.
It blocks both movement domains from foundation placement. Villagers build
from its land perimeter; Fishing Ships spawn and deposit at its water perimeter.
Ships share the incremental route planner's work, search and retained-frontier
caps with land units. Their water terrain adapter never reads or populates the
land route cache. Diagonals cannot cut land corners. Land movement retains its
existing terrain adapter and cache.

Movement cursors use the selected owned living units' movement domains. A
mixed land/naval selection shows move if any selected unit can enter the tile:
the server accepts or rejects each unit's move independently. Ships use ocean,
lake and river tiles except fords; land units retain land surface passability.
Resources and building footprints block both domains. Cursor feedback describes
the destination; the authoritative route planner still decides reachability.

Villagers may gather shore fish; ships may gather both fish kinds. Fishing is
an appended worker task. A full load returns to the nearest reachable eligible
owned completed building, with the existing bounded perimeter fallback. Ships
use Docks only; shore fishermen use Town Centers, Mills or Docks. Other villager
food jobs keep their existing drop-offs. Exhausted fish are removed through
the normal overlay feed and replacement searches remain local and bounded.

## Data and sprite sources

`make assets-dat-table` produces the ignored
`local-assets/dat-table/table.json`. Its `rows[].gameplay` records preserve
costs, training location/time, collision radii, resource storage/capacity,
speed, armour, work rate and task multipliers. No original DAT bytes, table
output, sprite pixels or derived images are committed.

The available local file is the Age of Kings trial's **VER 5.7**, not a DE DAT.
Its SHA-256 is `a213ed092071e1abd234181158c198f4a83ded970e308bf312eef66d752c3f2e`.
These measurements must not be represented as a decode of a current DE install.
The table below records the trial measurements. Current DE overrides are
listed separately; independent DE DAT qualification remains a limit.
The current DE building/unit cross-reference is
[Siege Engineers' extracted technology data](https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json).
The cursor layout was checked against
[genieutils Unit](https://github.com/Tapsa/genieutils/blob/master/src/dat/Unit.cpp)
and [Bird](https://github.com/Tapsa/genieutils/blob/master/src/dat/unit/Bird.cpp).

| DAT unit | Rules used | Sprite sources (trial SLP ids) |
|---|---|---|
| 45 Dock | 150 wood; 35 s; 1800 HP; radii 1.5×1.5 (3×3); trial LOS 6; armour 0/7 | 376 standing, 4397 foundation; optional DE `b_dark_dock_age1_x1.sld`, `b_misc_foundation_dock_x1.sld` |
| 13 Fishing Ship | 75 wood; 40 s at unit 45; trial 60 HP; LOS 5; capacity 15; speed 1.26; trial armour 0/4 | 444 idle, 449 moving, 446 fishing; five native rows mirrored to eight facings |
| 56 fisherman | work rate 0.43; capacity 10; shore fishing 0.43 food/s | 3575 working, 3980 carrying |
| 69 shore fish | 200 food | 3549, 34 animation frames |
| 53 Perch | 200 food | 420, 30 stored frames |
| 455 Dorado | 225 food | 1910, 49 stored frames |
| 456 Salmon | 225 food | 1911, 49 stored frames |
| 457 Tuna | 225 food | 1912, 49 stored frames |
| 458 Snapper | 225 food | 1913, 49 stored frames |

The trial ship work rate is 0.28: shore multiplier 1 and deep-fish class 5
multiplier 1.75 give 0.28/0.49 food/s. DE
[update 169123](https://www.ageofempires.com/news/age-of-empires-ii-definitive-edition-update-169123/)
reduces both fish rates by 15%, ship HP to 50 and armour to 1/1. The implemented
current rates are therefore **0.238 shore / 0.4165 deep food/s**; villagers
retain **0.43 food/s**. Ship progress uses integer ten-thousandths, avoiding
rounding drift; speed uses integer 126/100. Fishing Ships have no attack.
The extracted DE unit 45 supplies the implemented Dock LOS 8. This task does
not implement the naval update's Dock garrison/arrows or whaling mechanics.

The loader keeps bounded sampled fish and fisherman animation rows
in the fixed four-page atlas and retains complete source validation. Ship idle,
move and fishing graphics each have one native frame per facing. Fish animate
independently of moving units, including while the camera is stationary.
Ships and fishermen use the shared native actor path, including packed phase,
eight facings, mirrored rows and player colour. Fishermen sample every second
200 ms native work frame, preserving the loop duration within the fixed atlas.
Selected ships use native body bounds for picking and HP bars;
flat water does not occlude hull pixels below their ground anchor. Fish remain
neutral resource sprites, outside the unit-selection list.
Fog remembers fish as resources, retaining last observed amounts and dimming
every animation frame in explored tiles. Enemy ships require current sight.
Fishing Ships reveal a five-tile disk (DAT unit 13); Docks reveal eight tiles
using the DE rule above. Fish animation invalidates rendering only when the
displayed sampled frame index changes for visible fish with animated art.
Missing optional art remains missing; corrupt present sources fail startup.
Dock foundations use their native shore foundation, without painting dirt over
the water footprint.

## Reproducible coast

`make fishing-scenario` writes a synthetic 128-tile coast to
`.cache/fishing/map-packages` and its camera/resource coordinates to
`.cache/fishing/scene.json`. This is synthetic input, not geographic evidence.
The browser harness includes the same coast in every disposable server. The
village fishing spec builds a Dock, trains and orders a ship, observes a
15-food deposit and orders a shore fisherman. Captures go to
`.cache/fish-shots/`. For a local app use `make dev DEV_PORT=8094` with
`AOE_MAP_PACKAGE_DIRECTORY=.cache/fishing/map-packages`, the emitted hash as
`AOE_OPEN_MAP_PACKAGE`, and the private trial pack as `AOE_ASSET_PACK`.
