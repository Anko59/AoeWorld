# Village interface

The top bar shows the current player's wood, food, gold and stone, working
villager counts, population and the server's game clock. The current age follows the authoritative player state (Dark or Feudal). Objectives, Chat, Diplomacy and Settings are disabled because those
features have no gameplay implementation.

The bottom frame contains a five-by-three command grid, selection details and
the strategic minimap. Frames and interface SVG symbols are original AoeWorld
artwork. Original game sprites are optional local-pack assets, described in
[assets](assets.md), and are never shipped in Git or images.

Click a villager or scout to select it, drag a box to select an owned group, or
double-click a unit to select every owned unit of that kind on screen. A group
can contain both villagers and the scout. Right-clicking a resource tells its
villagers to gather; the scout keeps its current order. Right-clicking ground
moves the group. Move in the command panel arms a destination click; Stop
cancels each unit's current job and queued destinations. Carried loads remain
governed by the simulation's Stop rules.

Arrow keys pan the camera; alphabetic keys are reserved for commands. B opens
economic buildings, then A places a Farm when a completed Mill is available.
Ctrl+0–9 assigns owned living units to a local control group; 0–9 recalls it,
a second tap within 350 ms centers its centroid, and Shift+number adds the
current selection to that group. Editable fields and dialogs keep their keys.
Ctrl+right-click queues up to sixteen movement destinations, including the
active leg. Numbered flags and connecting lines show the authoritative queue
for selected units. Plain orders and Stop replace or clear it. A blocked leg
is skipped when reached; later valid destinations still execute deterministically.

Right-click on the minimap or the whole map sends the selection to that point,
as a right-click on empty ground does: units move, attack-move or patrol,
Shift or Ctrl queues, Alt moves a General alone, and production buildings set
a ground rally, which also ends an order left armed. It never attacks, gathers,
garrisons, repairs, trades or unloads, never moves the camera, and does nothing
outside the world. While an order is armed (Move, Attack move, Patrol, General
alone) the left button orders too if an own living unit or sheep is selected;
otherwise it ends that order and jumps. The latest eight orders are marked on the
map for 0.7 s: four ivory chevrons close on a move, four orange ones between the
axes on an attack-move, and two gold arrowheads part from a patrol. With reduced
motion they stand still for 0.9 s; Stop clears them. With a building placement or a far-view production
destination armed, a right-click anywhere on the map only cancels it. Cancelling
a placement also ends every other armed order.

Selected production buildings show their rally flag in every renderer, even
without optional flag art or beyond explored terrain. A local pack's original
hammer and sword cursors indicate construction/repair and hunting/attack;
authored cursors cover packs without those sheets. Right-clicking a damaged
own building repairs it, charging half its original cost over a full HP bar.

Click the Town Center footprint (or press H to select and center it), then press
Q or click Train Villager. The cost is fifty food and the authoritative training
time is twenty-five seconds.
The panel shows the queue and current progress. Cancel removes the active
entry and refunds food. Refusals explain ownership and resource failures in
plain words. The current rules start three villagers plus the scout. The HUD
shows living population / building space. Up to fifteen units can be queued per
building even at full space; training waits until more housing completes.
There is no global maximum population.

The top-left kingdom activity panel groups training queues and research from
all owned buildings, including those outside the camera view. Each row uses
the existing unit/technology art, a queued count and the furthest advanced
active producer's progress. Waiting units count but do not advance the bar.
Clicking a row selects and centres that producer; a tie prefers an active
producer, then the lowest building ID. The panel disappears when no work is
queued. It shows at most six types (fewer on short viewports), with "+N more"
for the remaining types. Its input is a bounded summary of existing building
snapshots; no new server stream or opponent information is sent.

Maps are created only through Single Player Game Setup. Menu offers Resume,
disabled Settings and Quit to Main Menu. Retained requests reuse their original
idempotency key; definitive refusals are cleared. Skirmish loading keeps Cancel.
Existing scene/minimap capture hooks remain available.
The scene also exposes actor coordinates for browser interaction evidence.
When villagers are selected, it exposes the nearest resident node per resource
kind in a nine-chunk window. Targets may be offscreen; they grant no simulation
authority.

Player/building snapshots and sparse deltas are retained independently of unit
interpolation. The server retains the bounded owned village outside the camera
view so selection and worker counts continue to follow authoritative state.
HTML presentation updates at most four times per second. A whole-map overview
suspends covered canvas presentations while input, terrain requests and the HUD
remain live; closing it resumes presentation. Capture hooks remain available. Missing
villager or Town Center art draws markers on every rendering backend. Imported player masks use the banner colour selected during game setup.

Validation uses the focused duel gate, WASM browser unit tests, the complete
Playwright suite and the unchanged WASM-size baseline. Synthetic browser maps
and fixture art do not qualify real-map play or finished RTS performance.

Village command feedback uses the simulation destination rule and composed terrain.
Resource hover highlights a gatherable node; selecting it shows remaining stock.
The WebGPU default is a narrow, warm silhouette rim with the contextual gather
cursor. `?hover=cursor` shows only that cursor, `?hover=outline` selects the
default rim explicitly, and `?hover=glow` uses a wider inner glow. Invalid or
absent flags select outline. The old ground circle is removed. Compatibility
backends retain contextual cursors. Selection ellipses are unchanged.

Classic AoE2's contextual gather cursors are the reference (moderate confidence
about hover-only decoration, based on prior play rather than a running classic
executable). DE definitely offers configurable hover highlights: its
[official accessibility guide](https://www.ageofempires.com/age-ii-de-accessibility/)
lists Off, Only Selections, Only Hovers and All. Confidence is high in that
setting's existence; this prototype does not claim to reproduce DE's exact
shader. DAT sprite outline masks alone cannot establish a hover interaction.

The effects reuse the final visible coverage attachment and existing instanced
contour pass. Cursor-only does not activate that pass. Outline/glow affect only
the hovered sprite's visible opaque silhouette, preserving its interior,
transparent holes and shadows. Hover picking stays on the existing four-Hz HUD
path; there is no new per-frame unit traversal or sprite packet allocation.

`browser/tests/proofs/hover-options.spec.ts` exercises all three resource kinds
and modes. Without local configuration it uses synthetic art; an ignored
`.cache/hover-options/capture.json` containing
`{"url":"http://127.0.0.1:8121/"}` switches it to an already running native-pack
`make dev` instance. The proof retains nine screenshots and served build IDs
under `.cache/hover-options/`; these files must never enter Git.
The minimap uses a gilded diamond frame with visible deposits and solid, outlined marks
for the known actors: own ones with a pale rim, and six or more joined buildings
of one player, walls and farms included, as a keep. Other players' marks lie over
own ones, so a raid on an own town is seen; own units lie over their buildings,
and units under the keep of their own town. A cell of both shows the other player
inside the own outline. Pings of orders lie over all marks. The cells are
anchored to the land, so they move with a zoomed minimap by whole px. Ivory
brackets of a constant page size locate the camera at every minimap zoom and menu
size. The closed outline inside them is the true view, and a view thinner than
4 px is a dot of at least 2 px; once it is larger than the brackets it carries
them on its visible corners, cut at the borders of the world and of the visible
map, which includes the bars beside a letterboxed one.
A view of the whole visible map is an outline just inside its border. An arrow
replaces a large view with no visible part, or a small one whose centre is off
the map. The marker lies above the map and below the buttons over it. A camera
jump closes the brackets on the new place once, a frame never over the map's
border; with reduced motion a second, still bracket shows for 0.7 s instead.

A new hit on an own unit or building that the screen does not show raises an
alert on the minimap: a hollow eight-point star for eight seconds, with three
beats of a ring around it (none with reduced motion), half as large again on the
whole map. Hits within 24 tiles of its first blow keep the star and raise no
other. The newest four battles have a star; up to 64 are remembered, and the
stalest one without a star makes room for another. A left click
within 8 px jumps exactly to a star, a right-click orders as usual, and a screen
reader hears "Under attack" at most once in ten seconds. Hits by own strikers are no
attack, nor are bites of game: the server tells animals near the camera only, so game
is every boar it ever told of and what an own villager was told to hunt. A boar bites
only who struck it. A wolf attacks unprovoked, so each of its bites is an alert, also
on a villager that fights it. A building lost within 12 s of its
last hit is an alert too, at its middle, and a wall or a gate also without one, as
one blow can take it. Nothing is told of what the player deleted or cancelled in the
last 5 s of game time, nor where an own building is told on the same footprint in
the same tick (stone over a palisade, a gate in a wall). Only the changes of a
tick are read, and a unit's hit only for one second of game time: neither a state
seen for the first time nor the snapshot of
a pan or a reconnection is an alert. On the detailed scene, hits inside the
canvas, also those behind a panel, are not told; in the far view and under the
whole map, where no unit is shown, all are.

Every land sample takes one colour from its own class and relief shade. Land in
sight is lifted inside a light rim, remembered land is dim, and unexplored land
is a dark chart drawn from position alone: the composed canvas stays empty in an
unexplored cell and fades inward only, so no scaling shows land under the fog.
An explored cell shows the sample over it: the coarse raster has one per 4x4
cells, so it can show land from outside the cell until the fine raster arrives.
Until a map's land arrives, or if it fails, explored cells show a flat tone.
With fog off the plain land is shown.
Grid remains available through its minimap button and G. Selection ellipses use
a continuous constant-pixel stroke across all renderers and zoom levels.


Select villagers and press B for economic buildings or V for military buildings.
The DE grid keys are House Q, Mill W, Mining Camp E, Lumber Camp R, Farm A,
Blacksmith S and Market D; military keys are Barracks Q, Range W, Stable E,
Outpost A, Palisade S, Tower F, Palisade Gate X, Stone Wall Z and Stone Gate G. Costs and availability are
visible; Feudal entries are grey with a "requires Feudal Age" tooltip in Dark
Age. Farms require a completed Mill.
Choose a building and click a green footprint to place it. Drag a palisade or stone wall
line from its first to last tile (or click its two endpoints); red previews
are refused locally. Escape or
right-click cancels placement. Right-click a foundation to resume construction,
or a completed Farm to assign its first selected villager as farmer.

Building selection shows hit points, construction and queue progress, Farm
food, and research. Q trains a Villager at the Town Center or Militia at the
Barracks. Town Center buttons research Loom, Town Watch and Feudal Age, with
costs and prerequisites enforced by the server. Camps and the Mill offer the
[Feudal economy technologies](economy/feudal.md), and the Town Center adds
Wheelbarrow. Construction and research can
be cancelled from the panel. Building art is described in
[Dark Age building art](assets/buildings.md).

Production lines show one current training entry. Research replaces the lesser
unit in that command slot, including Militia → Man-at-Arms → Long Swordsman,
Archer → Crossbowman, Spearman → Pikeman, Skirmisher → Elite Skirmisher,
Scout Cavalry → Light Cavalry and Galley → War Galley. Knight remains its own
Castle Age line. A researched technology gives its slot to its next step, or
leaves the panel when its line is complete.

Double-click an owned building to select its kind throughout the visible play
area. Group training uses the eligible selected building with the fewest queued
units, with entity ID breaking ties. Pending clicks count until acknowledged
state arrives, so a burst of clicks also spreads work between buildings.
