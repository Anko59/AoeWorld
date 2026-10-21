# Feudal buildings and economy

The owner asked: "if you have time you can continue with the fog of war,
fishing and the feudal age". This change is the economy/building part of Feudal;
The [military roster and Blacksmith](../military.md) and [fishing](../fishing.md)
extend these shared buildings and queues.
Feudal buildings integrate with the authoritative fog of war.

## Sources and reproduction

`make assets-dat-table-de` reads the owner's licensed install, mounted read-only,
at `resources/_common/dat/empires2_x2_p1.dat`. The measured layout is **VER 8.9**.
It writes ignored `local-assets/dat-table/feudal-de.json`, with the compressed
file's BLAKE3, record offsets, costs, collision radii, armour classes, research
locations, build-button slots, prerequisites and raw effect commands. No original data is committed.
The bounded survey implements the public
[Genie unit layout](https://github.com/SiegeEngineers/genieutils-py/blob/master/src/genieutils/unit.py).
It is a focused survey, not a complete DE dat decoder; other versions fail.

The independent
[DE dat extraction](https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json)
and the install's `resources/_common/dat/CivTechTrees/*.json` cross-check names
and availability. The licensed local version wins where extracted versions
differ: this install has an **850-HP, 35-wood** Watch Tower.

`make assets-dat-table` separately decodes the trial's VER 5.7 sprite families.
It now lists the Feudal units as well; Palisade Gate is explicitly absent there.

## Buildings

Times are game seconds for one builder; footprints are tiles. Armour is
melee/pierce. The simulation's common construction, placement, ownership,
refund and collision rules apply to all entries.

| Building (DE unit) | Wood/stone | Seconds | HP | Footprint | LOS | Armour |
| --- | --- | --- | --- | --- | --- | --- |
| Archery Range (87) | 175/0 | 50 | 1500 | 3×3 | 6 | 1/8 |
| Stable (101) | 175/0 | 50 | 1500 | 3×3 | 6 | 1/8 |
| Blacksmith (105, upgraded from 103) | 150/0 | 40 | 1800 | 3×3 | 6 | 1/8 |
| Market (84) | 175/0 | 60 | 1800 | 4×4 | 6 | 1/8 |
| Watch Tower (79) | 35/125 | 80 | 850 | 1×1 | 10 | 1/7 |
| Palisade Gate (792), Dark/Feudal | 30/0 | 30 | 240/400 | 4×1 | 6 | 0/0 |

The first five require Feudal. Range and Stable also require a completed
Barracks; Market requires a completed Mill. Their queues use the
[Feudal military roster](../military.md).
The Watch Tower is built directly: DE's tree links it to Outpost, but it does
not upgrade an existing Outpost. An ungarrisoned tower deals 5 pierce damage,
at range 1–8, every 2 seconds, through the existing combat hit/death path.
It queries visible enemy units through the spatial index, with canonical target ties.

Palisade Gate is available in **Dark Age** (the installed tech tree says age 1).
DE effect 293 scales its HP by 1.66667 at Feudal, giving 400 integer HP.
The current command supports its 4×1 orientation. It opens for nearby owned
units and remains open while any unit occupies it; other rotations and locking
are not implemented. Long-distance route planning still sees a closed gate as
blocked until a friendly unit approaches it.

## Market trading

A completed, owned Market sells and buys 100 units of wood, food or stone for
gold at the player's own price. Each player starts every commodity at 100 gold
per 100 units. **Buy** costs the price plus a 30% commission (100 -> 130 gold,
integer floor) and raises that price by 3; **sell** pays the price minus 30%
(100 -> 70) and lowers it by 3. Prices clamp to [20, 9999]. A trade needs the
gold (buy) or 100 units (sell); a non-Market, unfinished or foreign building is
refused, as are trades without enough resources; `RejectedNoMarket` is the wire
reason for a building that is not a Market. Prices live in each
`GameplayPlayerState` snapshot (append-only fields) and in the replay hash.
Only trades move prices; there is no passive recovery, trade cart, tribute or
Guilds fee reduction yet.

**Constants and sources.** The Genie dat carries no trading fields: the
`dat-table` survey in `local-assets/dat-table` and the SiegeEngineers
extractions cited above contain no price, commission or step values. The
values (100 base, 30% fee, step 3, floor 20, ceiling 9999) are DE's published
Market rules as recorded on the Age of Empires II wiki Market page; they could
not be cross-checked offline against the install, so treat the floor, ceiling
and step as documented-but-unverified and re-check against a DE game before
v1.0. They live in one table, `MARKET_RULES` (`economy/market/mod.rs`), mirrored
by `MARKET_MIN_PRICE`/`MARKET_MAX_PRICE` for wire validation.

The economic (B) and military (V) pages use the install's **definitive**
bindings in `resources/_common/dat/hotkeys.json`: Range W, Stable E,
Blacksmith S, Market D, Watch Tower F and Palisade Gate X. Build-button
indices in the dat provide a second cross-check.

## Technologies

Every listed technology requires Feudal, pays once on queueing and uses the
existing research cancellation, uniqueness and completion machinery.

| Tech (DE id/effect) | Building | Food/wood | Seconds | Effect |
| --- | --- | --- | --- | --- |
| Double-Bit Axe (202/195) | Lumber Camp | 100/50 | 25 | wood work rate ×1.20 |
| Horse Collar (14/14) | Mill | 75/75 | 20 | newly placed farms +75 food (175→250) |
| Gold Mining (55/55) | Mining Camp | 100/75 | 30 | gold work rate ×1.15 |
| Stone Mining (278/278) | Mining Camp | 100/75 | 30 | stone work rate ×1.15 |
| Wheelbarrow (213/200) | Town Center | 175/50 | 75 | speed ×1.10; carry limit ×1.2695 |

These are the actual effect multipliers in this install. Integer carrying uses
the ceiling of the fractional limit: 10→13. Speed changes 0.8→0.88 tiles/s;
gathering credits retain the exact fractional rates rather than rounding them
to whole hundredths. Existing and future villagers use the researched effects.
Horse Collar does not replenish existing farms. Town Watch is now correctly
age-gated too; Loom stays available in Dark Age.

## Age-up and art

Feudal upgrades existing and future House HP to 750, camp/Mill HP to 800 and
Barracks HP to 1500, with the upgraded LOS and armour from their DE records.
Damage is preserved; foundations gain only the HP earned by their work so far.

Each building carries its observed age for art selection. Remembered buildings
retain the variant last seen until revisited; an unseen age-up does not change it. Trial SLPs: Range **24**, Stable
**1009**, Market **2278**, Tower **2655**, House **2235**, Barracks **133**,
Mill body/sails **741/737**. The Mill uses these layered parts even with
a DE overlay: retaining 91 copies of the full DE body exceeds the fixed Objects
atlas page. Present DE sources are still validated before that choice.
Every building, construction and ambient source packs at full native resolution:
one atlas texel per source pixel, native world sizes, hotspots and animation
loops. There is no half-resolution policy and no per-source sampling step. Native
resolution needs 9,884,418 object texels, so the atlas grew from four to five
2048-square RGBA8 pages (83,886,080 bytes, +16 MiB; terrain pages 0-1, objects
pages 2-4). The numbers, the lossless reclaim that was measured first and the
memory cost are in [performance](../performance.md#native-building-atlas).
Blacksmith's **2263** is absent from the trial.
Feudal camp parent SLPs **3503/3491** are absent too, but their native body
deltas **3507/3495** are available. Both ages compose those DAT bodies; the
optional DE overlay supplies art only when native layers are absent.

`make assets-import-de AOE_ASSET_PACK=local-assets/packs/<trial-pack>` imports
the missing art and refreshes the generated local `buildings` overlay pointer.
It keeps both packs; it replaces no regular file. `make assets-verify` accepts
`AOE_ASSET_PACK` to verify one pack. The Feudal DE stems are
`b_west_{archery_range,stable,blacksmith,market,tower,house,mill,barracks,town_center,dock}_age2_x1.sld`.
The trial DAT's Feudal Dock is unit 133 (`DOCK2`); its West European
standing family `DOCK2NNW`, SLP 388, is present in the trial pack and supplies
the fallback when the optional DE age-2 Dock is absent.
The gate uses `b_dark_gate_palisade_se_{closed,open,constr,rubble}_x1.sld`.

DE graphic ids: Range 12, Stable 513, Blacksmith 53, Market 2268, Tower 4202,
Gate 8208; Feudal TC 3253, House 2209, Mill 368, Barracks 93, camps 3118/3133.
The DE Town Center standing SLD contains the complete building. Gameplay prefers
DAT compositions for every available Dark and Feudal family, including the
Feudal Town Center's age-specific annex graphics and the Dock's native body,
shadow and wake. Missing ambient parent art does not hide available DAT deltas.
The Palisade Gate is absent from the trial DAT and retains its DE overlay;
additional gate orientations remain separate work. Foundations and one-tile
aprons cover land only, including all Feudal buildings.

## Local demo

`POST /maps/reset?scenario=feudal-economy` is an explicit development setup:
Dark Age, 10,000 of each resource, a completed Mill, Lumber Camp and Barracks.
The game's one-Hz clock runs under the server's ordinary 20-Hz ticker (20×).
Advance, build and research through the ordinary UI. `/maps/reset` restores the
normal game; an unknown scenario returns 400 without replacing it.
The village e2e and server scenario use this path and the real command codec.
It is synthetic functional evidence, not finished RTS performance qualification.
