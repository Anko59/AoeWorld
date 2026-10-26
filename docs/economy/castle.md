# Castle Age

The generic Castle roster has no civilisation bonuses or unique units.
Research Castle Age at an owned, completed Town Center with an empty queue:
800 food, 200 gold, 160 game seconds, Feudal Age, and two distinct completed
kinds from Archery Range, Stable, Blacksmith and Market. Towers and duplicate
buildings do not count. Earlier age unlocks remain available.

## Authoritative data

Numeric values come from the owner's licensed DE **VER 8.9** DAT, source BLAKE3
`ee111552cc11cfbf535a5e4bc28adb7137ce4b2eee68fd2faf3bac96f9235c9e`.
`make assets-dat-table-de` reproduces the bounded survey into ignored
`local-assets/dat-table/feudal-de.json`. DAT research 102 retains the internal
name “Feudal Age”, but its effect is named “Castle Age”; its cost and time are
800 food, 200 gold and 160 seconds. Only the generic effects are implemented.

`make build-wasm` generates ignored `web/pkg/content.json` from the native
simulation tables and reviewed actor catalog. Startup loads this single current
asset before creating the scene: unit stats, prices, technology requirements,
building footprints and actor graphic ids do not need compiled browser copies.
The native simulation still owns balance and research effects. Projection tests
compare the runtime records to those tables; malformed or stale content fails
startup. The `content-source` Cargo feature supplies canonical fixtures to
browser tests and is absent from the production build.

| Building / DAT id | Cost | Seconds | HP | Tiles |
| --- | --- | --- | --- | --- |
| Castle / 82 | 650 stone | 200 | 4800 | 4×4 |
| Monastery / 104, upgraded to 31 | 175 wood | 40 | 2100 | 3×3 |
| University / 209 | 200 wood | 60 | 2100 | 4×4 |
| Siege Workshop / 49 | 200 wood | 40 | 1500 | 4×4 |

The Workshop needs a completed Blacksmith. The current DE Workshop has 1500
HP; the trial graphics DAT has older numeric balance and supplies art only.
A completed Castle supplies 20 population slots (DAT resource storage 4).
Castles fire five 11-pierce arrows every two seconds with range 1–8. Murder
Holes removes the minimum range; Fletching and Bodkin Arrow each add one attack,
range and LOS. No garrison system exists in this checkout, so garrison is not
part of this milestone.

| Unit / DAT id | Food / wood / gold | Seconds | HP | Attack | Melee / pierce armour |
| --- | --- | --- | --- | --- | --- |
| Long Swordsman / 77 | 50 / 0 / 20 | 21 | 60 | 9 | 1 / 1 |
| Pikeman / 358 | 35 / 25 / 0 | 22 | 55 | 4 | 0 / 0 |
| Crossbowman / 24 | 0 / 25 / 45 | 27 | 35 | 5 pierce | 0 / 0 |
| Elite Skirmisher / 6 | 25 / 35 / 0 | 22 | 35 | 3 pierce | 0 / 4 |
| Knight / 38 | 60 / 0 / 75 | 30 | 100 | 10 | 2 / 2 |
| Light Cavalry / 546 | 80 / 0 / 0 | 30 | 60 | 7 | 0 / 2 |
| Monk / 125 | 0 / 0 / 100 | 51 | 30 | healing | 0 / 0 |
| Mangonel / 280 | 0 / 160 / 135 | 46 | 50 | 40 melee | 0 / 6 |
| Battering Ram / 35 | 0 / 160 / 75 | 36 | 175 | 2 melee, +150 buildings | −3 / 180 |

Unit upgrades replace the line's current trainable identity and upgrade existing
living units, preserving damage taken. Queued units change identity and retain
their fraction of training progress. Costs, research times and prior technology
requirements are in `research/castle.rs`. This includes Hand Cart, Town Patrol,
the five second-tier Blacksmith technologies, Heavy Plow, Bow Saw, Gold Shaft
Mining, Masonry, Ballistics and Murder Holes. Heavy Plow adds 125 farm food and
one farmer carrying slot. Gather credits use millionths so the two mining
upgrades retain their exact 1.15 × 1.15 multiplier. All arithmetic remains integer.

## Mechanics and limits

Mangonel projectiles move at the DAT's 3.5 tiles/second and hit their fixed aim
point after travel. Their one-tile blast queries the local unit index and
building-footprint index, includes friendly fire, and uses deterministic full
and half unit-damage bands. Siege-class and building bonuses use the same
armour/death path as other attacks. This is a bounded generic splash model,
not a complete emulation of Genie projectile fragments or blast falloff.

Arrows and javelins can miss a moving target. Ballistics leads the target using
its fixed-point velocity at launch; later changes of direction can still dodge.
Masonry adds 10% building HP, 1 melee/pierce armour and 3 building-class armour,
preserving existing damage. Farms, palisade walls and gates are excluded.

Idle Monks automatically heal the nearest injured owned non-siege land unit
within four tiles at 2.5 HP/second at the normal tick rate. Entity identity breaks
distance ties; healing never exceeds max HP. The simulation indexes Monks at
spawn/removal and uses local unit queries. **Conversion is phase 2.** Healing
orders, relics and faith are not included.

## Presentation and AI

Unit and building art comes from the local trial DAT/SLPs and licensed DE camp
overlay. Castle architectural families follow effect 102. DE camps reuse their
age-two standing art; the install has no separate age-three camp source. The measured trial
University and Castle Blacksmith composite/animation back references emit each
layer once; unrelated cycles still fail. Icons use native DAT icon indices in
the existing interface sheets. No original pixels are stored in Git or images.

Castle actor poses retain native facings and timing while sampling at most two
cells per facing. Stand/decay use one. The existing 5120-frame, five-page atlas
budget remains unchanged. Absent optional original art retains semantic markers;
a present incomplete source remains an error.
Castle Mill and Blacksmith ambient layers use two native poses to keep their
large rectangles within the existing three object pages; construction is complete.

The Python AI saves for Castle at 32 villagers once two qualifying kinds stand
and a twelve-unit Feudal army is established. It reserves prerequisite wood,
drains the Town Center queue, raises a Stable while advancing, then trains Knights. It researches
Crossbowman and Pikeman and keeps its economy and army managers active in Castle.

Research state now has 64 bits for the 33 technologies. Postcard varints retain
small-value encodings; tests pin Castle orders and the high bit. Replay pins are
regenerated because the canonical state hashes now include the wider bitset and
the ordered healer index, plus the finer gather-credit precision. This is one
current schema, with no migration path.

Functional gates and synthetic workloads do not qualify finished RTS performance,
Firefox on the owner's GTX 1660 Ti, or dedicated hardware.

Local original-art captures use `make dev DEV_PORT=8109` with the imported pack.
Set ignored `.cache/ui/proof.json` to
`{"phase":"castle","url":"http://127.0.0.1:8109/"}` and run `make ui-proof`.
The proof uses ordinary age, training and construction commands and saves PNGs
plus observed state only under `.cache/ui/`. Stop this server with `make down`.

The same build step emits the two specialized WebGPU shader modules into
ignored `web/pkg/*.wgsl`. WebGPU loads these exact build-assembled sources
before pipeline creation; WebGL2 retains its own shaders and needs no download.
Native and browser tests use the same generated modules, with no alternate
shader implementation.
