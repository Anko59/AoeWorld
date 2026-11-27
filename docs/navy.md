# Rivers and navy

River and lake banks now receive deterministic shore fish, including narrow
channels. Villagers work from a reachable adjacent bank. Channel fish require a
two-tile wet cross and remain ship-only resources. Placement is generation
recipe 5; regenerate local map packages instead of reading an older recipe.

A Dock needs a free 3×3 footprint crossing dry land and navigable river, lake or
sea water. Its water perimeter trains Fishing Ships and Transport Ships in the
Dark Age and the first tier of every warship line from Feudal Age; see
[the overhaul rules](#naval-warfare-overhaul-rules) for the tiers and techs.

Select up to five land units and right-click an owned Transport Ship. The
board cursor and **Board Transport Ship** hint identify the target. Units and
ship meet at the nearest reachable land shore; boarding happens on arrival,
not at order time. The ship shows its passenger count both above the hull and
in the selection panel. Passengers leave the map and stop providing vision or
accepting orders. Select the loaded ship and right-click a dry shore to sail
there and unload, or choose **Unload passengers** and then the shore. Explicit
Move and Ctrl-queued Move retain ordinary movement behavior. Ownership,
capacity and landing terrain are authoritative. Sinking a loaded transport
kills its cargo. A new Move or Stop cancels a pending rendezvous.
Transport orders use the ordinary Rust and Python SDKs too. Shore discovery
examines at most 4,096 land/water states and chooses the shortest combined
approach with one boarding transition; subsequent movement uses the
normal incremental route scheduler. This is one crossing order, not a
persistent automatic ferry route.

Every water tile is navigable by ships, including coastal shallows and the
marsh-textured narrow streams that land units can wade through. Ships use
water navigation, including diagonal bank-corner checks and
Dock footprint blocking. Galleys fire at ships and reachable shore targets.
Fire ships attack at short range. Demolition hulls detonate once and disappear;
their blast damages nearby enemy units and buildings.

The example AI uses only public explored terrain: villagers approach bank fish,
and known river food can prompt one Dock, one Fishing Ship and two Galleys.
Fleet and search bounds live in `ai/aoe_ai/config.py`.

## Naval Warfare Overhaul rules

Rules follow DE update 169123 (17 Feb 2026) with the numbers of the DE
update 188329 DAT, from
[the patch notes](https://www.ageofempires.com/news/age-of-empires-ii-definitive-edition-update-169123/),
[the naval warfare article](https://www.ageofempires.com/news/a-new-naval-warfare-in-age-of-empires-ii-definitive-edition/)
and the [aoe2techtree](https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json)
data (DAT unit ids in `navy/stats.rs`). Every ship now deals either pierce
(Galley line) or melee damage (fire, demolition, Hulk and cannon ships), and
there is one bonus class, Long Ranged Ship (armour class 60), carried by the
Galley line and the cannon ships. Galleys do +5 (Galleon +9), fire ships +1
against it; the Hulk line takes +1 against the Fire line's own armour class
(41), so Hulks counter fire ships and galleys counter Hulks. Ship armour
classes otherwise carry no bonus; Spearmen keep their anti-ship bonus.

| Line (Dock) | Feudal | Castle: Medium Warships | Imperial: Heavy Warships |
|---|---|---|---|
| Galley | Galley 110 HP | War Galley | Galleon |
| Fire | Fire Galley | Fire Ship | Fast Fire Ship |
| Hulk (melee armour) | Hulk | War Hulk | Carrack |
| Demolition | Demolition Raft | Demolition Ship (tech) | Heavy Demolition Ship (tech) |
| Cannon | | | Cannon Galleon, Elite (tech); Catapult Galleon |

Medium Warships (the former War Galley tech) is 150 wood 100 gold, Heavy
Warships 400 wood 315 gold. The Demolition Ship, Heavy Demolition Ship and Elite
Cannon Galleon techs are separate Dock techs (costs: `research/navy.rs`).
Training a lower tier after its upgrade queues the upgraded ship. Cannon
Galleons need no Chemistry. Blast radii are the DAT 2.5, 3 and 3.25 tiles.

University techs: Careening (Castle) and Dry Dock (Imperial) +1 pierce armour
each for all ships; Clinker Construction (Castle) and Carvel Hull (Imperial)
+10% speed each; Siphons (Castle) and Incendiaries (Imperial); Shipwright
(Imperial) ships cost 20% less wood and train 50% faster. Dock techs: Fishing
Lines (Feudal) and Gillnets (Castle, needs Fishing Lines) +10% gather rate and
+5 carried each. Forging, Iron Casting and Blast Furnace give the Hulk line
+1/+1/+2 attack; Fletching and Bodkin Arrow reach the Galley line as before.

Fish Trap (DAT 199): 100 wood, 40 s, 250 HP, armour 1/1; only Fishing Ships
build it (select ships, then the Fish Trap button, then open water) and work it,
by sitting on it, at the ship's own 0.35 food/s with Fishing Lines and Gillnets
applying. It holds a limited amount of food and collapses when spent; a
spent trap is rebuilt by a ship, never reseeded at a Mill. The Fish Trap's
sprite is not imported yet; it borrows the Farm's.

Stated assumptions, not in the sources: a trap holds 715 food (the DAT in
data.json gives no amount; 715 is the long-standing AoE II figure) and a ship
gathers from it at 0.35 food/s (the 21 per minute of the Conquerors era); the tier ages (Castle for second
tier, Imperial for third; Cannon Galleon Imperial as before the overhaul);
Gillnets in Castle Age; Siphons give the Fire line +1 attack and Incendiaries
make a sunk Fire Ship deal 30 melee damage within two tiles (the notes give
"explosive charge" and "detonate when sunk" with no figures). The Hulk's
-3 against standard buildings is not modelled (a negative class bonus cannot
reduce below the base hit in the current table). Fishing Ships garrisoning in
Docks and Whales are not part of this track; the Trade Cog is track D.

## Original data and art

The local original DAT is VER 5.7, SHA-256
`a213ed092071e1abd234181158c198f4a83ded970e308bf312eef66d752c3f2e`.
`make assets-dat-table` records its decoded numeric evidence outside Git.
Galley is DAT 539, War Galley 21, Fire Ship 529, Demolition Ship 527 and
Transport Ship 545. These are original trial measurements, not a claim of
current DE balance qualification.

Ship body art is selected from the owner's licensed DE installation:
`u_shp_galley_x1.sld`, `u_shp_war_galley_x1.sld`,
`u_shp_fire_ship_x1.sld`, `u_shp_demo_ship_x1.sld` and
`u_shp_transport_ship_x1.sld`. Eight client facings sample the original sixteen
directions through five stored mirrored rows. These static bodies share the
existing bounded atlas; separate wake, attack and sinking layers are not yet
animated. Import with `make assets-import-de`. All original bytes, decoded
pixels, packs and screenshots remain ignored local data.
