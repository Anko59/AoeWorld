# Imperial DE art inventory

The current importer selects `GAMEPLAY_BUILDING_IDS`, follows each selected
unit's standing graphic, DAT deltas and annexes, and imports their complete
native timelines, including all fourteen Imperial building records below.
The client selects separate Castle and Imperial DAT identities through compact
age and render metadata tables, for completed buildings and construction alike.
The simulation already updates every owned building's observed age when research
finishes; client scene caching consumes those building updates.

## Installed Western architecture

The licensed VER 8.9 DAT and installed SLD inventory select the following
Imperial records. These are semantic DAT identities, not guessed sprite offsets.

| Building | Imperial DAT unit | Standing graphic / source family |
| --- | --- | --- |
| Town Center | 142 | 3041 / `b_west_town_center_age4_back` |
| House | 465 | 2223 / `b_west_house_age3` |
| Barracks | 20 | 105 / `b_west_barracks_age3` |
| Mill | 131 | 380 / `b_west_mill_age3` |
| Lumber Camp | 565 | 3118 / `b_west_lumber_camp_age2` |
| Mining Camp | 587 | 3133 / `b_west_mining_camp_age2` |
| Dock | 51 | 236 / DAT delta composition |
| Archery Range | 14 | 24 / `b_west_archery_range_age3` |
| Stable | 153 | 525 / `b_west_stable_age3` |
| Blacksmith | 19 | 65 / `b_west_blacksmith_age3` |
| Market | 137 | 3434 / `b_west_market_age4` |
| Monastery | 32 | 150 / `b_west_monastery_age3` |
| University | 210 | 581 / `b_west_university_age4` |
| Siege Workshop | 150 | 489 / `b_west_siege_workshop_age3` |

Castle unit 82 selects graphic 174, `b_west_castle_age3`; the installed Western
set has no separate age4 Castle, House or Barracks SLD. An Age III versus IV
comparison should therefore preserve those silhouettes. Town Center, Market and
University have distinct age4 sources. A Town Center is a composition: its back
slice alone is not the building. Its main, front, centre and shadow slices must
continue to come from the DAT delta and annex resolver.

The construction mappings retain each selected age's DAT composition instead of
substituting Feudal foundations for Castle buildings. Observed building updates
invalidate the existing scene cache; an age change adds no per-frame building
scan and never substitutes a live enemy owner's age for remembered observations.

Native pixel chunks stream on visible demand. Startup resolves building animation
cells through a manifest index rather than repeatedly searching the entire unit
catalog. Future animation poses wait until a populated gameplay scene has been
presented before prefetching; cold requests therefore serve visible pixels first.
Metadata remains eager and bounded by the pack validator.

The opt-in `imperial-art` panel proof drives normal research, movement and building
commands against a local licensed pack and saves private screenshots and video.
`imperial-load-compare` alternates three fresh browser contexts per pack selection,
with identical camera and world. Its first-landscape measurement reads actual
world pixels and includes screenshot latency; a presentation counter alone can
describe an earlier empty frame. Optional `imperial-load-de-profile` also saves a
Chromium CPU profile. These local proofs are separate from dedicated hardware or
finished RTS performance qualification. No imported pixels or private packs
belong in Git.

## Local art check, 2026-11-03

The regenerated trial, DE world and DE actor packs passed `make assets-verify`.
The focused asset tests passed 132 tests; `make test-wasm` passed 668 tests.
The native-pack browser proof completed ordinary Imperial research and inspected
all ten Imperial unit tiers and their DE portraits. Town Center, Market and
University changed between Ages III and IV; House, Barracks and Castle retained
their installed Western silhouettes. Siege Workshop upgrades occupy a separate
command row from training and have a focused click regression test.

The private `.cache/showcase/imperial-art.webm` is a 28-second cut of the actual
browser recording. First, middle and last frames show the Castle Age Town Center,
native research icons and the Imperial Onager respectively. Its building pairs,
menus and ten unit closeups were compared with the inspected proof stills.

Three alternating fresh Chromium contexts per selection measured first visible
landscape at 19.91 seconds median without DE (12.70–21.59) and 15.15 seconds with
DE (14.27–17.82). These software-WebGPU upper bounds include screenshot latency,
share server and OS caches, and are not a controlled performance benchmark.
Only 33 native chunks, about 34 MB of the 1.63 GB payload, were fetched in each
DE startup capture. Native metadata remains eager.

Headed Firefox 159 rendered the Imperial village using WebGPU on the NVIDIA
offload path. `nvidia-smi` identified its WebDriver process on the GTX 1660 Ti
with 495 MiB allocated; the WebGL privacy bucket names a different card family.
Captured console logs contained the core/default-limits adapter message and no
GPU errors. The native screenshot used about 274 MB of GPU textures and 4.6 MB
of buffers. This is a local rendering observation, not hardware qualification.
The earlier owner's black-world failure remains unconfirmed. A separate white
ground failure in fog-disabled diagnostic worlds was reproduced and fixed by
requesting native terrain for their chunk-free geometry.
