# Game data file (`empires2.dat`)

`make assets-dat-table` decodes `local-assets/trial/Data/empires2.dat` and
writes `local-assets/dat-table/table.json` and `table.md`, both ignored and
local only. It prints the same table (ids and counts, no imagery) and
cross-checks the sprite ids the catalog uses. Nothing derived from the file is
committed; the tests use synthetic fixtures and, when the trial file is
present, one real-file test of the ids below.

## Format as measured

The file is raw DEFLATE (no zlib header); inflated it is the genie layout,
version string `VER 5.7` (1,504,094 bytes in the trial). Only that version is
accepted; another one fails with an unsupported-version error rather than a
guess. Sections, in order:

| Section | Size rule |
| --- | --- |
| Version | 8 bytes |
| Terrain restrictions | u16 restrictions, u16 terrains, two u32 pointer arrays, 20 bytes per terrain per restriction |
| Player colours | u16 count, 36 bytes each |
| Sounds | u16 count; 4 bytes, u16 files, 4 bytes, 23 bytes per file |
| Graphics | u16 slots, i32 pointer per slot (zero is absent), then 78-byte records, 16-byte deltas, 12 bytes per angle if attack sounds are used |
| Terrain block, random maps, effects | skipped |
| Unit headers | u32 slots, per slot a flag, then u16 tasks of 59 bytes |
| Civilisations | u16 count, then Gaia: marker 1, name[20], resources, u16 unit slots, i32 pointers, unit records |

The skipped middle is not parsed. The civilisation block is found by a
structural scan; the unit headers are the one offset whose walk ends exactly at
that block. `Report` names each offset, so a drift shows as a stop reason, not a
wrong table. Unit records: a fixed prefix, damage sprites (5 bytes each), the
name, then blocks by genie type (speed, dead-fish, bird, combat, creatable
65 bytes, building 80 bytes). Every slot id must equal its index.
Types 20 and 25 carry the speed block only; 40 and 50 are unverified because
the Gaia set has none.

## Findings on the trial (Gaia, 5,024 graphics, 720 units)

- The graphic id is not the SLP id: graphic 1284 `VMBAS_FN` uses SLP 1479.
- Villager idle 1479 and walk 1484 are unit 83. Town Center 891 is graphic
  `RTWC1NNG`, the stand graphic of unit 618 (the Dark Age Town Center variant);
  unit 109 uses the later-age composite.
- The catalog's cavalry 3004/3008 is `LTCAV_*`, Light Cavalry (units 546 and
  575). Scout Cavalry (unit 448) is `SCOUT_*`, SLP 2085 idle, 2089 walk.
- Villager task art comes from unit headers: `proceeding`, `working` and
  `carrying` slots. Wood 1535/1536 and stone 1560 and carrying 1552 match.
  1545 is `VMLUM_TN` (lumberjack), not gold mining; gold miners reuse 1560 with
  carry 2117. 1490/1493 are `VMBLD_DN`/`VMBLD_FN` (builder dying and standing),
  not berry foraging; the forager is `VMFOR_*` with tasks 2589/2592.
- Farm, Lumber Camp and Mining Camp art is partly or wholly absent from the
  trial `graphics.drs`; the table's `pack` column says which.

Limits: only Gaia is decoded (other civilisations choose architecture sets);
display names come from a fixed id list because the language DLLs are not read;
techs, effects and per-age graphic upgrades are not decoded.
