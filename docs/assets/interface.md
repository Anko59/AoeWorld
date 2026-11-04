# Native DE interface

Original interface pixels remain exclusively in ignored `local-assets/` packs.
`make assets-import-de` exports native DE unit, building and technology icons,
commands, resource counters, bottom panels and the menu background into the
private overlay's `interface.json`. The server's existing `/asset-pack/` mount
serves its atlas pages. No original PNG, screenshot, DDS or SLP belongs in Git
or a Docker image.

The UI catalog contains checked atlas rectangles. DE DDS and PNG pixels retain
their alpha; base trial UI pages composite the existing colour, player and
shadow planes using the current blue player colour. Original planes remain
unchanged. The DOM's UI images do not occupy the gameplay GPU atlas.
The DOM loads its pages once, hydrates icons once and uses cropped CSS atlas
backgrounds for the panel compartments. A missing catalog or unavailable page
retains the authored SVG interface. Selection and queue icons are rebuilt only
when their identities or counts change; progress values use the normal bounded
HUD refresh, never the renderer's frame loop.

The client merges all five overlay groups: `units`, `technologies`, `commands`,
`resources` and `panels`. Overlay-local page paths remain under `buildings/`.
Each group replaces matching base roles independently, so equal numeric unit,
building and technology sheet indices never share the wrong image.

## Current DE inventory

| Role | Installed source and selection |
| --- | --- |
| Unit icons | DAT icon indices in `widgetui/textures/ingame/units`; includes the spawned army, workers, wildlife and resource selection portraits |
| Building icons | All 55 gameplay building records, excluding the six bridge segments, select DAT icon indices in the separate `buildings` directory |
| Technologies | All 53 current technologies select DAT research icon indices in the separate `tech` directory |
| Commands | Native `actions/NNN_.png`: cancel 0, delete 1, ungarrison 2, stop 3, garrison 4, repair 28, economic build 30, military build 31, move 32, line 35, attack 41, box 43, staggered 62, flank 63, auto-scout 74 and attack-move 84 |
| Counters | `icons/resource_{food,wood,gold,stone}.png` and `icons/pop.png` |
| Bottom compartments | `panels/WEST/command-panel_extended.png`, `single-selection-panel.png`, `map-panel.png` and `topbar.png` |
| Button and portrait frames | `panels/blank_icon.png` and `panels/frame_regular.png` |
| Menu, setup and loading background | `widgetui/textures/backgrounds/mainmenu_bg.dds`, converted privately to the bounded menu image |

The HUD has no civilisation identity yet. Both existing style keys use the DE
WEST architecture. Stone Wall 117, Stone Gate 64 and Palisade Gate 792 resolve
native building icons 31, 36 and 44. Resource portraits use DAT units 59, 349,
66, 102 and 53, with native unit-sheet indices 6, 32, 14, 9 and 276. Bridge
commands use DE's generic build icon; General-only movement uses the General's
native portrait. Formations and auto-scout use native command icons.

PNG decoding accepts bounded images up to 2048 per side and 4 Mi-pixels.
Ordinary DDS icons keep their 256-pixel side limit. The menu adapter
separately validates a block-aligned BC1 source up to 8192×4096, rejecting cubes
and volumes, then averages each 4×4 block directly into one RGBA output pixel.
The installed 5120×2160 painting becomes 1280×540 without allocating its full
source-resolution pixel image. This does not raise the full DDS decoder cap.
The browser crops the menu's atlas rectangle once into a private in-memory PNG
URL, so unrelated pixels on that atlas page cannot enter the menu background.

## Base pack inventory

`make assets-import` also writes a base `interface.json` from the trial's
`interfac.drs`. The complete DE overlay replaces these visible interface roles;
the base catalog remains available to partial-pack inspection and fixtures.

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

The base bottom strip starts at y=596, height 172. Its compartments span
x=0..276, 276..680 and 680..1024. These rectangles are scaled independently for
modern viewport widths.

## DAT mapping

The licensed DE unit, building and research indices select their own sheets.
Research language identities locate the supported VER 8.9 records, whose signed
icon fields select the installed DDS numeric prefixes. Missing or ambiguous
cells fail import. The DE common unit reader preserves the signed icon field
at byte 51 of the supported unit record.

The unit field is signed `icon_id` at byte 47 of the measured VER 5.7 unit
record. Negative IDs mean no icon. Building type 80 uses the building sheet;
other unit types use the unit sheet. The decoder now walks all civilisations
to reach the research table, preserving research slot ID, name, location and
signed icon ID. The trial contains 426 research slots. The following tables
describe that base trial catalog rather than a separate current DE mapping.

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
AoK has no dedicated Move command button; its base catalog uses the pointer
cursor. The complete DE overlay supplies its native Move command instead.

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
Focused `de::icons::ui::tests` exercise PNG alpha and bounds, exact sheet/frame
selection, atlas crop bounds and streaming menu conversion without changing the
DDS cap. The village interface-art browser spec supplies synthetic metadata and
SVG pixels to exercise all five overlay groups, menu crop pixels, fallback,
selection DOM retention and the Barracks queue's actual unit identity.

`make ui-proof` runs the local real-server capture spec. It requires ignored
`.cache/ui/proof.json` containing `{"phase":"after","url":"http://127.0.0.1:8141/"}`.
Start the selected pack with `DEV_PORT=8141 AOE_ASSET_PACK=local-assets/packs/<hash>
make dev`; capture Town Center, villager, multi-selection and a constructed
Barracks training Militia. Screenshots and observed state go to `.cache/ui/`.
Stop the server with `make down`. Capture baseline files directly from a saved
checkout before switching to the new UI; intercepting an HTML navigation can
make Chromium's local-network checks reject its WebSocket.

## Data limits

The portrait uses the native unit/building icon and imported portrait
border: no separate large character portrait is exported. Garrison, repair and
other command art is inventoried without adding gameplay actions the client
does not implement. Original fonts and unrelated menu screens are not exported
as browser UI resources. Cursor installation retains its existing 64-pixel
browser limit; the larger native command PNGs do not replace those cursors.
