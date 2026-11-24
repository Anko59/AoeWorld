# Roadmap: map and world generation

Goal (owner 10-11): as realistic as possible while totally playable. Issue 265 list is fully merged.

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Map look from Definitive Edition's full variety: trees, soils, plants, terrain textures, vegetation, wildlife and animals (the map was designed with original-edition assets) | 11-25 x1 | PLANNED | Owner 11-25: "De offers a lot more variety ... we need to explore and take everything we can" | Survey DE's terrain, tree, plant and animal sets; map each to biomes and geodata classes; import with the DE pipeline |
| Provinces auto-generated from geodata (basins, ridges, coasts, 100-300 tiles) | 10-31 | PLANNED | Part of [hierarchy](hierarchy.md); no code | With the office engine |
| Random-map realism polish after the 265 list (rivers and ground) | 10-14 | PARTIAL | Everything in issue 265 merged (#336 #348 #350 #284 #289 #307 #309 #374 and more); polish moved to a consolidated list that is now closed | Only act on new owner reports |
| Territorial biome study: derive biomes from real-world data | 10-11 x1 | UNVERIFIED | Proposal recorded in issue 274 and the 265 list ticks it; no merged PR adopts it | Ask whether the proposal was accepted or dropped |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Rivers with banks and fords, shorelines, cliffs, snow, rocks, ground dressing, blending, minimap, grid | 10-11 | DONE | #336, #348, #350, #284, #289, #307, #309, #311, #368, #370, #374 |
| Fords at regular intervals; smallest rivers dropped as maps shrink | 11-12, 10-29 | DONE | #801, #637 |
| Natural, fair starts | 11-09 | DONE | #810 |
| Mekong (Asia rivers, sea stays sea) | 11-11 | DONE | #787 |
| Map preparation faster; one never-resetting progress bar | 11-06 | DONE | #725, #726, #717 |
