# Geodata sources, licences and where packages may live

Owner's request (2026-11-26): "all of it on real maps that are really in the game, no more
synthetic maps that are just used for CI". The pinned map is the owner's Anatolia request
([`qa/anatolia-map.json`](../../qa/anatolia-map.json)); `make game-map` makes sure its package
exists and prints its hash and directory ([workflow](workflow.md)).

## Sources of that map

The licence column is the string recorded in the code (`crates/geodata/src/source_catalog`);
nothing here is legal advice. The attribution column says what the named licence asks of anyone
who shares the data or a work made from it. The code records no credit line, so the exact wording
is still to be confirmed by the owner before any package is shared.

| source | recorded licence | attribution the licence asks for |
| --- | --- | --- |
| ETOPO 2022 v1, 60 arc-second surface (NOAA) | NOAA public domain | none required; credit NOAA NCEI as courtesy |
| Natural Earth 5.1.1, 1:10m land | Natural Earth public-domain terms | none required; credit Natural Earth as courtesy |
| Potential biome v0.2, Zenodo record 3526620 (raster and class table) | **CC BY-SA 4.0** | credit the authors, link the licence, state changes; **share-alike**: an adaptation is shared under the same licence |
| HYDE 3.2.1, DANS-25G-GEZ3 (baseline, general supplementary, README) | dataset metadata CC0; release README CC BY 3.0 | credit the HYDE authors and cite the dataset for the README terms |
| HydroLAKES v1.0 | HydroSHEDS license; CC BY 4.0 product attribution | credit the HydroSHEDS / HydroLAKES authors, link the licence |
| HydroRIVERS v1.0 Europe and Middle East (the region that holds Anatolia) | HydroSHEDS license for scientific, educational, and commercial use | credit HydroSHEDS as that licence states |

Two more layers exist in the code for small maps only (a footprint of at most 96 one-degree
tiles; Anatolia needs more): Copernicus GLO-90 ("Copernicus DEM public COG licence") and ESA
WorldCover 2021 ("CC BY 4.0; ESA WorldCover attribution required"). The Anatolia package uses
neither. Package manifests list the lock of every source they used (`source_locks`).

## Where generated packages and sources may be kept

- **Caches only.** CI keeps the generated package in the GitHub Actions cache and nothing else.
  It is never a release asset and never a workflow artifact; the sources are never uploaded
  anywhere. A package carries data derived from the CC BY-SA 4.0 layer, so handing it to anyone
  (a release, an artifact that every reader of the repository can download) is a redistribution
  with share-alike and attribution duties.
- **Publishing a package or the sources needs the owner's decision.** No agent, workflow or
  pull request makes it. If the owner decides to publish, this page is where the attribution text
  and the licence of the published package are written first.
- Locally, `.cache/` and `local-assets/` stay out of Git ([workflow](workflow.md)).
