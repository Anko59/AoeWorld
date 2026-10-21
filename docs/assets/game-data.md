# Game data file (`empires2.dat`)

`make assets-dat-table` decodes `local-assets/trial/Data/empires2.dat` and
writes `local-assets/dat-table/table.json` and `table.md`, both ignored and
local only. It prints the same table (ids and counts, no imagery) and
cross-checks the sprite ids the catalog uses. Nothing derived from the file is
committed. `make test-assets-dat` runs synthetic fixtures and explicitly sets
`AOE_TRIAL_DAT` for the real-file checks; a requested missing file fails.
Ordinary unit tests skip those checks when the variable is unset.

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
| Terrain directory | 140-byte map/tile-size header, 32 records of 416 bytes; remaining terrain block, random maps and effects skipped |
| Unit headers | u32 slots, per slot a flag, then u16 tasks of 59 bytes |
| Civilisations | u16 count, then Gaia: marker 1, name[20], resources, u16 unit slots, i32 pointers, unit records |

The remaining middle is not parsed. The civilisation block is found by a
structural scan; the unit headers are the one offset whose walk ends exactly at
that block. `Report` names each offset, so a drift shows as a stop reason, not a
wrong table. Unit records: a fixed prefix, damage sprites (5 bytes each), the
name, then blocks by genie type (speed, dead-fish, bird, combat, creatable
65 bytes, building 80 bytes). Every slot id must equal its index.
Types 20 and 25 carry the speed block only; 40 and 50 are unverified because
the Gaia set has none.

Each 16-byte graphic delta is `graphic_id i16`, padding i16, runtime sprite
pointer i32, `offset_x i16`, `offset_y i16`, `display_angle i16`, padding i16.
Signed offsets, angle and list order are retained; padding and runtime pointers
are discarded. Graphic layers are retained, including shadow layer 5.

AoK's building tail has **no snow graphic**. Its offsets, checked against raw
trial bytes and the GenieUtils AoK serializer, are:

| Offset | Field |
| --- | --- |
| 0 | construction graphic i16 |
| 2, 3, 5 | adjacent mode u8, graphics angle i16, disappears u8 |
| 6, 8, 10, 12 | stack unit, foundation terrain, old terrain, research (i16 each) |
| 14 | can burn u8 |
| 15–54 | four annexes: unit i16, NE displacement f32, SE displacement f32 |
| 55, 57, 59, 61 | head unit, transform unit, transform sound, construction sound (i16 each) |
| 63, 64, 68 | garrison type u8, heal rate f32, repair rate f32 |
| 72, 74–79 | pile unit i16, six loot bytes |

The importer carries local building recipes in `manifest.json`: stand graphic
and recursive deltas, annex displacements, native layer order, collision
footprint, construction recipe, foundation id and resolved terrain SLP.
The terrain's dimensions also supply foundation sheet topology. The Farm's
terrain 7 / SLP 15004 uses its native 6×6 sheet through the same terrain mixer;
Palisades and Docks declare no foundation; gameplay buildings without a native
foundation use the shared dirt material. Unfinished Farms use dirt until
completion, when their ploughed sheet and optional DE texture take over. The additional sheet stays inside
the existing two terrain pages and uses one more material directory entry.
Native NE/SE displacements project to `((NE+SE)*48, (SE-NE)*24)` source pixels.
Annexes sort by `SE-NE` (isometric depth); equal-depth units and same-layer
deltas retain DAT order, while lower graphic layers precede the bodies.
That depth rule is an engine reconstruction, not a claim of original-executable
trace equivalence. No part is rotated, mirrored or manually elevated.
Unavailable resources stay explicit in the recipe and are skipped; a declared
present but missing/corrupt frame fails startup. The head's primary stand
identity is retained: if that body is absent, the optional DE overlay supplies
its complete sprite with native anchors. Available trial compositions remain
in charge of their deltas and annex positions; missing optional shadows do not
suppress an otherwise available building. Farm texturing, rubble, the builder
animation and all UI/placement behavior share the current building path.

The manifest also stores `building_data`, the compiler's bounded hex startup
view of those same recipes. Native verification regenerates and compares it.
Its only current layout is little-endian: building count u8; per building,
unit i32, footprint 2×f32, foundation SLP i32 (-1 absent), sheet dimensions
2×u16, then complete/construction lists. Each list starts with u16 count;
each part is frame-zero manifest index u32 (`u32::MAX` absent), frame count
u16, angle count u16, primary-stand flag u8 (0/1), frame duration f32,
pixel offset 2×i32 and tile displacement 2×f32. The full bounded frame/angle
prefix is validated. Construction progress chooses a native angle; palisade
connectivity chooses its orientation. Static House variants follow entity id,
while Mill sails use the native frame duration.
Indices remove repeated strings from browser startup without changing offsets,
draw order or animation. The ten buildings in the current gameplay protocol,
including Dock and its native wake animation, occupy the live atlas. Foundation
terrain and its apron paint land samples only; water keeps its authored sheet
and surface metadata beneath completed Docks and construction sites.
The packing proofs validate full source prefixes before applying the client's
existing idle and fisherman selections: 5/50 Cavalry Standing cells, 5/75
Villager Standing cells, and 70/140 fisherman cells. This removes 185 unused
entries from the complete catalog proof. The Dock's native 60-cell wake at
150ms per cell is sampled at every second cell: 30 cells at 300ms retain the
nine-second loop. Static Dock art and all construction angles stay complete,
as do the 43 Mill sail cells and the Farm terrain sheet. The limits remain
5,120 selected entries and four
2048-square RGBA8 pages (67,108,864 bytes, or 64 MiB).
The reader bounds the bytes/counts and rejects
truncation, trailing bytes and invalid frame indices. Regenerate packs after a
format change; there is no compatibility reader.

`make assets-building-preview AOE_ASSET_PACK=local-assets/packs/<hash>` writes
the pack's Town Center recipe at native scale to `.cache/tc-shots/tc-composite.png`.
`BUILDING_PREVIEW_RECIPE=<local JSON>` and `BUILDING_PREVIEW_OUT=.cache/...png`
allow a local comparison recipe. Proof pixels stay ignored.
`make building-proof BUILDING_PROOF_REQUEST=<local JSON>` prepares local proof
captures: `maps` isolates one package, `pack` checks gameplay atlas placement,
`camera` reads the live Town Center, and `house` places a House through the
normal controller/build-order protocol. `village` gathers wood from the real
starting resources and builds a House, Mill, Barracks and Farm through those
same orders. `servers` serves the already-built
comparison apps on 18086/18087 and initializes their villages through gameplay;
it stops after 30 minutes or when `.cache/tc-shots/stop-proof-servers` exists.
Its optional `base` request selects an already-built comparison revision,
defaulting to `origin/dev`. Both villages include House, Mill, Barracks and Farm.
This is needed for building comparisons because spectator-only map captures
do not create a player or a starting Town Center.
After the tool has been built, `make building-proof-run` repeats its local
protocol preparation without rebuilding; it accepts the same request file.

For animated villages, the opt-in `tests/proofs/building-village.spec.ts`
browser proof reads `.cache/tc-shots/capture-request.json` with `url`, optional
`fog`, the required `revision`, and the Town Center's `x`/`y`. Set `revision`
to the exact `build` string returned by that server's `/health` endpoint,
including `-dirty` when present; the proof also compares the served WASM bytes
with this checkout. For example, replace the revision below with that build:

```json
{"url":"http://127.0.0.1:18089/","x":498,"y":3005,"revision":"<exact /health build>","fog":false}
```

It requires all five completed building
kinds in the wide view, native art, loaded terrain and the correct backend,
then captures the wide view and a Town Center close-up at the read-back camera after
a newly presented frame. Outputs are `final-village.png`,
`final-tc-close.png` and, with `fog: true`, `final-fog.png` under
`.cache/tc-shots/`. This captures actual animation; it does not claim the
unchanged-frame verdict of `make compare`, which cannot settle a visible
continuously animated Mill. Keep that refusal explicit in the local report.

## Findings on the trial (Gaia, 5,024 graphics, 720 units)

- The graphic id is not the SLP id: graphic 1284 `VMBAS_FN` uses SLP 1479.
- Villager idle 1479 and walk 1484 are unit 83. Town Center 891 is graphic
  `RTWC1NNG`, the stand graphic of unit 618 (the Dark Age Town Center variant);
  unit 109 is the multipart head: its stand is the support assembly and its
  annexes include unit 618. The head supplies the dirt foundation and shadow.
- The catalog's cavalry 3004/3008 is `LTCAV_*`, Light Cavalry (units 546 and
  575). Scout Cavalry (unit 448) is `SCOUT_*`, SLP 2085 idle, 2089 walk.
- Villager task art comes from unit headers: `proceeding`, `working` and
  `carrying` slots. Wood 1535/1536 and stone 1560 and carrying 1552 match.
  1545 is `VMLUM_TN` (lumberjack), not gold mining; gold miners reuse 1560 with
  carry 2117. 1490/1493 are `VMBLD_DN`/`VMBLD_FN` (builder dying and standing),
  not berry foraging; the forager is `VMFOR_*` with tasks 2589/2592.
- Farm, Lumber Camp and Mining Camp art is partly or wholly absent from the
  trial `graphics.drs`; the table's `pack` column says which. The composition
  section also inventories every delta and annex resource separately.
- TC terrain 27 resolves to SLP 15006 (grass/dirt), used across its 4×4
  footprint plus a one-tile margin with the ordinary terrain border blend.
  The margin is a renderer choice; the decoded building tail has no such field.
  Construction sites use
  the same foundation. SLP 890 is referenced on layer 10 but absent; the eight
  available TC sprites render without it, matching the trial's inventory.

A stage that cannot be validated (for example unit headers that no offset
reaches) sets `report.stopped`; the command still writes the table, marked
`INCOMPLETE` in `table.md` and `status` in `table.json`, and exits non-zero.
Only a missing `graphics.drs` means "no pack"; any other read error fails.

Limits: only Gaia is decoded (other civilisations choose architecture sets);
display names come from a fixed id list because the language DLLs are not read;
techs, effects and per-age graphic upgrades are not decoded.
