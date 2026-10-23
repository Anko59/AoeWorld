# Original in-game interface

Original interface pixels remain exclusively in ignored `local-assets/` packs.
`make assets-import` imports `trial/Data/interfac.drs` alongside the gameplay
archives and writes `interface.json` beside the atlas manifest. The server's
existing `/asset-pack/` mount serves both. No original PNG, screenshot or SLP
belongs in Git or a Docker image.

The UI catalog contains rectangles in private `interface-*.png` pages, composited
from existing colour, player and shadow planes using the current blue player
colour. Original planes remain unchanged. These UI pages do not
increase the five-page gameplay atlas or add interface frames to that atlas.
The DOM loads its pages once, hydrates icons once and uses cropped CSS atlas
backgrounds for the panel compartments. A missing catalog or unavailable page
retains the authored SVG interface. Selection and queue icons are rebuilt only
when their identities or counts change; progress values use the normal bounded
HUD refresh, never the renderer's frame loop.

## Trial inventory

All resource numbers below identify SLPs in `interfac.drs`:

| SLP | Content | Selection |
| --- | --- | --- |
| 51121 | Western stone/column HUD, 1024×768 | Command, parchment selection and minimap compartments; top texture and population icon |
| 51133 | Celtic orange HUD, 1024×768 | Same compartment layout |
| 50706 | 52 building icons | DAT building `icon_id` is the frame index |
| 50730 | 103 unit/resource icons | DAT unit `icon_id` is the frame index |
| 50729 | 106 technology icons | DAT research `icon_id` is the frame index |
| 50721 | 69 command icons | Stop 3, economic buildings 30, military buildings 31, repair 28, ungarrison 2, cancel 0 |
| 51000 | 19 cursor frames | Pointer 0 for Move, sword 4 for attack, garrison 13 |
| 50732 | Four resource icons | Wood 0, stone 1, food 2, gold 3 |
| 53011 | Western button tiles and small indicators | Frame 0 is the empty stone button |
| 50601 / 50606 | Blue / gold button tiles | Imported, available for local inspection |
| 50751 | Toolbar and frame pieces | Frame 36 is the selection portrait border |
| 50745 | 26 health-bar states | Imported; live health/progress values remain semantic HTML progress controls |

The native bottom strip starts at y=596, height 172. Its compartments span
x=0..276, 276..680 and 680..1024. These rectangles are scaled independently for
modern viewport widths. The current game does not expose civilisation identity
to the HUD; Western is the default. `data-interface-style="celtic"` on the root
selects the other catalogued frame before initialisation.

## DAT mapping

The unit field is signed `icon_id` at byte 47 of the measured VER 5.7 unit
record. Negative IDs mean no icon. Building type 80 uses the building sheet;
other unit types use the unit sheet. The decoder now walks all civilisations
to reach the research table, preserving research slot ID, name, location and
signed icon ID. The trial contains 426 research slots.

| Entity | DAT unit ID → icon frame |
| --- | --- |
| Villager / female villager | 83 → 15 / 293 → 16 |
| Scout / Militia / Man-at-Arms | 448 → 64 / 74 → 8 / 75 → 10 |
| Archer / Skirmisher / Spearman | 4 → 17 / 7 → 20 / 93 → 31 |
| Fishing Ship | 13 → 24 |
| Town Center / House / Barracks | 109 → 28 / 70 → 34 / 12 → 2 |
| Mill / Lumber Camp / Mining Camp | 68 → 19 / 562 → 40 / 584 → 39 |
| Farm / Palisade Wall / Outpost / Dock | 50 → 35 / 72 → 30 / 598 → 38 / 45 → 13 |
| Archery Range / Stable / Blacksmith | 87 → 0 / 101 → 23 / 103 → 4 |
| Market / Watch Tower | 84 → 16 / 79 → 25 |

| Technology | DAT research ID → icon frame (sheet 50729) |
| --- | --- |
| Loom / Town Watch / Feudal Age | 22 → 6 / 8 → 69 / 101 → 30 |
| Double-Bit Axe / Horse Collar | 202 → 70 / 14 → 2 |
| Gold Mining / Stone Mining | 55 → 15 / 278 → 87 |
| Wheelbarrow / Man-at-Arms | 213 → 79 / 222 → 85 |
| Forging / Scale Mail / Scale Barding | 67 → 17 / 74 → 63 / 81 → 66 |
| Fletching / Padded Archer Armor | 199 → 34 / 211 → 49 |

Command assignments were cross-checked against
[freeaoe ActionPanel.h](https://github.com/sandsmark/freeaoe/blob/master/src/ui/ActionPanel.h).
AoK has no dedicated Move command button; our existing Move control reuses its
original pointer cursor. Garrison is cursor frame 13, rather than the gate
command icon.

The full local mapping, including wildlife and Feudal building variants, is in
`interface.json`. `make assets-dat-table` also records unit and technology icon
IDs in ignored local JSON. Research record layout was cross-checked against
[genieutils Research.cpp](https://github.com/Tapsa/genieutils/blob/master/src/dat/Research.cpp);
the unit field against
[genieutils Unit.cpp](https://github.com/Tapsa/genieutils/blob/master/src/dat/Unit.cpp).

## Validation and local proof

`make test-assets-ui` runs synthetic decoder, sheet selection and crop bounds
tests. `make test-assets-ui-local` checks the owner's trial DAT mappings against
reviewed numeric IDs; it requires the local trial and embeds no original bytes.
The village interface-art browser spec supplies synthetic metadata and SVG
pixels to exercise pack hydration, fallback, selection DOM retention and the
Barracks queue's actual unit identity.

`make ui-proof` runs the local real-server capture spec. It requires ignored
`.cache/ui/proof.json` containing `{"phase":"after","url":"http://127.0.0.1:8141/"}`.
Start the selected pack with `DEV_PORT=8141 AOE_ASSET_PACK=local-assets/packs/<hash>
make dev`; capture Town Center, villager, multi-selection and a constructed
Barracks training Militia. Screenshots and observed state go to `.cache/ui/`.
Stop the server with `make down`. Capture baseline files directly from a saved
checkout before switching to the new UI; intercepting an HTML navigation can
make Chromium's local-network checks reject its WebSocket.

## Data limits

Only Western and Celtic complete gameplay HUD frames were found in this trial.
It has no Palisade Gate unit 792, so that command retains its authored fallback
icon. The portrait uses the native unit/building icon and imported portrait
border: no separate large character portrait was found. Garrison, repair and
other command art is inventoried without adding gameplay actions the client
does not implement. Original fonts and unrelated menu screens are not exported
as browser UI resources.
