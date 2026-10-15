# Village interface

The top bar shows the current player's wood, food, gold and stone, working
villager counts, population and the server's game clock. The current age is
Dark Age. Objectives, Chat, Diplomacy and Settings are disabled because those
features have no gameplay implementation.

The bottom frame contains a five-by-three command grid, selection details and
the strategic minimap. Frames and interface SVG symbols are original AoeWorld
artwork. Original game sprites are optional local-pack assets, described in
[assets](assets.md), and are never shipped in Git or images.

Click a villager or scout to select it, drag a box to select an owned group, or
double-click a unit to select every owned unit of that kind on screen. A group
can contain both villagers and the scout. Right-clicking a resource tells its
villagers to gather; the scout receives a Move order. Right-clicking ground
moves the group. Move in the command panel arms a destination click; Stop
cancels the job using a Move to each unit's current authoritative position.
Carried loads remain governed by the simulation's Move rules.

Click the Town Center footprint (or press H to select and center it), then press Q or click Train Villager. The
cost is fifty food and the authoritative training time is twenty-five seconds.
The panel shows the queue and current progress. Cancel removes the active
entry, refunds food and releases its reserved population slot. Refusals explain
food, population, ownership and resource failures in plain words. The current
rules start three villagers plus the scout, with five total population slots.

Menu opens Resume, disabled Settings, Quit to Main Menu and separate map tools.
Until a start menu exists, Quit reloads the page. The map creator is a separate
overlay opened from Menu, and the minimap's navigation controls remain beside
it in the bottom frame. Existing scene/minimap capture hooks remain available.
The scene also exposes bounded actor and nearest-resource coordinates for
browser interaction evidence; they grant no simulation authority.

Player/building snapshots and sparse deltas are retained independently of unit
interpolation. HTML presentation updates at most four times per second; the
renderer, capture counters and camera retain their existing cadence. Missing
villager or Town Center art draws markers on every rendering backend. The
current setup has no player-colour choice, so imported player masks use blue.

Validation uses the focused duel gate, WASM browser unit tests, the complete
Playwright suite and the unchanged WASM-size baseline. Synthetic browser maps
and fixture art do not qualify real-map play or finished RTS performance.
