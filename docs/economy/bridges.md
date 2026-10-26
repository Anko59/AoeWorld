# Buildable bridges

Bridges are an AoeWorld extension, available to villagers from Feudal Age.
Select villagers, open Economic buildings (`B`), choose Bridge (`G`), and drag
between two explored land banks. Two clicks also set the endpoints. Escape
cancels placement. The preview uses resident, explored terrain; the server
validates and charges the complete order atomically.

A span follows either world tile axis, with one to forty water tiles between
its land endpoints (three to forty-two footprint tiles). Every interior tile
must be lake, ocean, river or shallow water; fords, intermediate islands, resources,
units, other buildings, unexplored tiles and occupied bridge spans are refused.
Diagonal and bent spans are refused. Reversed drags have identical footprints.

The cost is **25 wood and 5 stone per water tile**, with **8 seconds of work per
water tile** for one villager. A four-tile crossing costs 100 wood and 20 stone
and takes 32 seconds before walking time. Multiple builders use the ordinary
construction formula. Each bridge has **1,200 HP**, 2 melee armour and 5 pierce
armour. These are authored extension values, not claimed AoE II balance values.
Cancellation refunds unfinished work proportionally; repair costs half the
whole span's original cost for a full HP bar.

Foundations block passage. Completion opens the deck for land units, along its
axis only. Land units enter and leave at the banks; lateral and diagonal entry
or exit on the water deck is forbidden. Ships cannot pass through a completed
deck. Destroying the bridge removes the entire crossing; land units on its
water deck die and their deaths appear in that tick's updates. Units on its
land endpoints survive.

Navigation keeps the immutable package snapshot. Sparse, copy-on-write deck
entries participate in existing chunk collision fingerprints. Completion and
destruction rebuild local component labels and border portals lazily under the
ordinary deterministic preparation budgets. Unaffected static chunk graphs
remain reusable; no package-wide flood or regenerated snapshot is needed.

The public Rust SDK adds `Order::BuildBridge`, and Python adds
`Client.build_bridge(builders, x1, y1, x2, y2)`. Completed bridge observations
update the SDK's known land routes, including directional edges and removals.
The example AI can enable `Config(build_bridges=True)` (default false). It
tries a crossing toward a known enemy building after ordinary land routing
fails, checks at most five nearby cardinal spans, requires explored water and
banks plus a reachable villager, and waits thirty seconds between attempts.
This heuristic is deliberately local and does not guarantee a crossing exists.

## Original scenario art

The AoK `VER 5.7` DAT contains the following scenario definitions. The local
trial's `graphics.drs` omits their SLPs, so the licensed DE overlay supplies the
corresponding scenario sprites. When original SLPs exist, they take priority.
Only identifiers and importer code are tracked; pixels and packs remain private.

| Piece | AoK unit | Graphic | SLP | DE source stem |
|---|---:|---:|---:|---|
| Bridge A top | 605 | 3331 | 3688 | `s_bridge_a_top` |
| Bridge A middle | 606 | 3333 | 3690 | `s_bridge_a_middle` |
| Bridge A bottom | 607 | 3335 | 3692 | `s_bridge_a_bottom` |
| Bridge B top | 608 | 3337 | 3694 | `s_bridge_b_top` |
| Bridge B middle | 609 | 3339 | 3696 | `s_bridge_b_middle` |
| Bridge B bottom | 610 | 3341 | 3698 | `s_bridge_b_bottom` |

A pieces follow world Y; B pieces follow world X.
DE pack sources are `de/<stem>_x1.sld`, frame 0. Runtime slots 45–50 retain six
frames in the existing atlas. The source pieces are displayed at half scale
for the single-lane tile cadence; the deck renders behind crossing actors.
Missing art retains semantic markers. Import with `make assets-import-de`;
verify DAT identifiers with `make test-assets-dat`. `make test-bridges` covers
the focused simulation, navigation, protocol, SDK and catalog regressions.
