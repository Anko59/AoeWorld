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

This inventory and implementation are not runtime qualification. Pack
regeneration, runtime tests, close in-game screenshots and the requested age-up
video remain separate qualification. No imported pixels or private packs belong
in Git. The opt-in `imperial-art` panel proof drives normal research and building
commands against a local licensed pack and saves private screenshots.
