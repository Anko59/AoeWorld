# Rivers and navy

River and lake banks now receive deterministic shore fish, including narrow
channels. Villagers work from a reachable adjacent bank. Channel fish require a
two-tile wet cross and remain ship-only resources. Placement is generation
recipe 3; regenerate local map packages instead of reading an older recipe.

A Dock needs a free 3×3 footprint crossing dry land and navigable river, lake or
sea water. Its water perimeter trains Fishing Ships and Transport Ships in the
Dark Age, and Galleys, Fire Ships and Demolition Ships after Feudal Age.
Castle Age is researched at the Town Center; War Galley is researched at a Dock
and upgrades existing, queued and future Galleys.

Select up to five land units and right-click an owned Transport Ship. The
board cursor and **Board Transport Ship** hint identify the target. Units and
ship meet at the nearest reachable land shore; boarding happens on arrival,
not at order time. The ship shows its passenger count both above the hull and
in the selection panel. Passengers leave the map and stop providing vision or
accepting orders. Select the loaded ship and right-click a dry shore to sail
there and unload, or choose **Unload passengers** and then the shore. Explicit
Move and Shift-queued Move retain ordinary movement behavior. Ownership,
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
Fire Ships attack at short range. Demolition Ships detonate once and disappear;
their blast damages nearby enemy units and buildings.

The example AI uses only public explored terrain: villagers approach bank fish,
and known river food can prompt one Dock, one Fishing Ship and two Galleys.
Fleet and search bounds live in `ai/aoe_ai/config.py`.

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
