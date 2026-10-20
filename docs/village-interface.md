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
cancels the job using a Move to each unit's current authoritative position.
Carried loads remain governed by the simulation's Move rules.

Click the Town Center footprint (or press H to select and center it), then press
Q or click Train Villager. The cost is fifty food and the authoritative training
time is twenty-five seconds.
The panel shows the queue and current progress. Cancel removes the active
entry, refunds food and releases its reserved population slot. Refusals explain
food, population, ownership and resource failures in plain words. The current
rules start three villagers plus the scout, with five total population slots.

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
The minimap uses a diamond frame with own units, buildings and visible deposits.
Grid remains available through its minimap button and G. Selection ellipses use
a continuous constant-pixel stroke across all renderers and zoom levels.


Select villagers and press B to open the building menu. It lists each Dark Age
building with a hotkey, cost and availability. Farms require a completed Mill.
Choose a building and click a green footprint to place it. Drag a palisade
line from its first to last tile (or click its two endpoints); red previews
are refused locally. Escape or
right-click cancels placement. Right-click a foundation to resume construction,
or a completed Farm to assign its first selected villager as farmer.

Building selection shows hit points, construction and queue progress, Farm
food, and research. Q trains a Villager at the Town Center or Militia at the
Barracks. Town Center buttons research Loom, Town Watch and Feudal Age, with
costs and prerequisites enforced by the server. Construction and research can
be cancelled from the panel. Building art is described in
[Dark Age building art](assets/buildings.md).
