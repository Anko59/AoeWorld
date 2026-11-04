# Strategic play

Below sprite zoom, a General has a selectable army token with its authoritative
soldier count, actual unit mix, stance and current state. An arrow follows its
first reported movement destination. Click selects without changing zoom and
opens the ordinary Captain/Trainer panel. Right-click ground uses the ordinary
orders, including Ctrl for waypoints and Alt to move the General alone. Visible
enemy Generals show an unknown count and no private composition or stance.
Other actor clusters still zoom into their location. Returning to sprite zoom
hides tokens and restores ordinary selection and rendering.

The on-screen layer buttons also accept one key while the strategic view is
active. Dialogs and editable controls retain their keys.
The developer timing overlay uses F8 here and retains F3 at sprite zoom.

| Key | Layer | Available data |
| --- | --- | --- |
| F1 | Ownership | Coloured building plots and the actual domains of active Castles |
| F2 | Armies | Army tokens and unit clusters |
| F3 | Economy | Steward cards and their Town Center domains |
| F4 | Threats | Historical enemy sightings, labelled with age, for two minutes |
| F5 | Resources | Discovered deposits, retained across camera subscriptions and terrain eviction |
| F6 | Roads & bridges | Known bridges; road-network data is unavailable |

Steward cards select the same Town Center as near zoom. They show the server's
managed/resident counts and growth wait reason, plus observed food, wood, gold,
stone jobs and idle workers identified by their authoritative manager. Job
counts describe the current authorized unit stream; they are not an invented
global census. A question mark means the report is unavailable. An ungarrisoned
Steward's domain is inactive. Selecting its Town Center opens its resource weights, cap and training controls.

## Work bounds

The existing server stream bounds unit and building residency and includes own
economy units outside the camera region. Enemy presentation additionally checks
current fog visibility. New summaries are built at the four-Hz HUD cadence, never
by scanning all players on every animation frame. The client forwards numeric
records; JavaScript aggregates worker jobs and order destinations to conserve
WASM space. Territory geometry and unchanged DOM content are cached.

Presentation is capped at 256 General records, 64 Town Center records, 256 bridge
records, 1,024 historical contacts and the existing 2,048 deposit markers. Nearby
labels use a bounded screen-grid placement search and retain lines to their true
locations. These bounds are not a qualification of a crowded 1,000-player RTS.
The strategic scene retains one terrain raster and fog plane. The cartographic
surface is baked once per map, capped at 1,024² RGBA pixels (4 MiB), from the
server's existing 512² terrain classes and DEM-derived relief shades. Water and
forests retain those measured classes; fields and grazing retain the generator's
historical land-use classifications. Soil alone never becomes farmland. No
heights, cities, road connections or political boundaries are invented.

At most 2,048 authorized building footprints feed farm plots, owner-coloured plot
borders and 192 retained settlement symbols. Plot paths are batched by owner
colour and rebuilt only when the HUD records change. Capital/Castle symbols take priority
when icons share a screen cell. Architectural symbols distinguish capitals,
Castles, towers, mills, barracks and harbours; the near scene and cockpit retain
original game art. Enemy buildings require explored terrain; the server's authorized
building stream remains the source. The legend gives a scale along a world tile
axis. Empty near-view command/selection compartments make room for land; selecting
an entity still opens its ordinary controls. `data-frame-ms` measures strategic
CPU drawing; `data-terrain-build-ms` measures the one-time cartographic bake.
Driver/compositor memory and time are additional.

Zooming does not
request detailed terrain for the whole map. While it covers the detailed scene,
the retained renderer canvas is transparent but remains the input surface. The
strategic cover is removed only after a complete near frame is presented,
including when terrain or art delays that replacement.

Resource discovery retains exact resource IDs in a 2,048-entry browser cache.
Accepted authoritative resource updates retire depleted IDs incrementally;
paged snapshots commit their removals only when complete. New discoveries pause
during a partial snapshot. A camera move neither clears known deposits nor
reveals unscouted ones. World replacement clears the cache.

The camera focus can move outside the world, keeping map edges inspectable.
Zoom preserves the ground under the pointer even when that requires an outside
focus. Only the visible tile request is clipped to the map bounds.

## Kingdom cockpit

The far view has an original-art kingdom panel with held resources,
population used/cap, recent credited deliveries, occupied building tiles and
living owned military/support units by type, including embarked/garrisoned units and
commanders. Villagers, Fishing Ships and Vassals are excluded from the army.
The General token counts its soldiers; the kingdom total also counts the
General himself. A missing economic report is shown as `?`.

Collection numbers are actual deposits in the simulation's twelve live
five-second buckets (55–60 seconds), without extrapolation. Spending, trade,
refunds and stock edits do not count as collection. Occupied land sums living
owned building footprints, including foundations and excluding water bridges;
it does not claim territorial borders or Castle-domain ownership.

## Saved views and jumps

At far zoom, Ctrl+F1–F4 save the camera position and zoom for this world;
Shift+F1–F4 recall them. The on-screen Save/View buttons do the same.
Plain F1–F6 still control layers. H uses the existing main-base command.
End jumps to the largest reported owned General army (soldier count, then
lowest General ID). Space jumps to the newest observed damage to an owned
unit or building. These jumps preserve zoom; a saved view restores zoom.
Missing targets stay disabled. Bookmarks and alerts clear when the world resets.
Typing in a form or opening a dialog suppresses these shortcuts.

## Units here

In the far-view production form, choose a type and a count from 1 to 10,
press Units here, then click ground. Esc, right-click, leaving far zoom or
losing focus cancels the destination. Feedback counts server-accepted training orders. A rejected rally sends no
training to that producer. Each batch includes at most 20 commands, matching
the human admission burst; large or unaffordable requests can be partial.

Eligible producers must be owned, alive, complete and able to train the type
at the reported age/research/resources. The split chooses the nearest ready
producer by squared distance to the destination, balancing queues among equally
near producers and breaking remaining ties by building ID. It uses farther
producers only when nearer queues have no space.
It includes pending training, reserves resources and population, and bounds
the burst including one rally per chosen producer. Every chosen producer's
rally is changed to the clicked ground. Existing queued units share that
producer rally. Missing economic reports disable the action.

The authoritative census is initialized on demand and then maintained by unit
births, deaths, upgrades and garrison transfers. Reports reuse it across ticks;
with fog enabled, only connected viewers receive their own player report.
`make perf-census-instructions` counts the serial report kernel over 20 calls
with 100,000 units, 1,000 economy players and zero, one or 1,000 viewers. Setup,
the initial census, simulation advancement and transport are outside the count.

## Building from the cockpit

At far zoom, choose a building in the kingdom panel, press Build here and
choose its site. The ordinary footprint, exploration, terrain, age, resource
and prerequisite checks apply, including wall and bridge spans. Only chunks
under the proposed site load at far zoom. Red footprints explain a blocked or
unavailable site; green is a local preview, not server acceptance.

Each grouped order sends up to four suitable villagers within 64 tiles of the
site. A site without nearby available workers explains why it cannot recruit. Free idle
workers take priority, then other free workers; managed workers can answer
when no free workers are available. Workers already building, fighting or
hunting are excluded. Ties use entity ID. No selection or zoom change is needed.
Feedback says Submitting until the ordinary server acknowledgement arrives.
One build can be pending at a time. A connection reset clears its pending
admission and reports that the previous order was not confirmed; inspect the
site before retrying. The Build here icon retains its DOM node across HUD
updates so a held pointer can complete its click. The client tracks outgoing commands against
the human burst of 20 and refill of 10 per second; the server retains authority.
Esc, right-click, focus loss and returning to near zoom cancel placement.

## First far view

The first zoom-out in each match pulls back from the village for 3.2 seconds,
reveals the wider land, then brings in the cockpit at 4.4 seconds. It completes
at 5.8 seconds. The already-presented village scales and fades over the strategic
plane; the detailed renderer remains retained without rebuilding a moving terrain view.
The transition never subscribes to detailed terrain across the whole map.
Skip or Esc completes the moment immediately. Pointer, wheel, keyboard input or
focus loss ends it and returns control at once. Direct far-view links and
reduced-motion preferences preserve the requested camera and show a brief title.
The simulation keeps running throughout. A session-stored digest of
the match resume identity prevents replay on reload or reconnect; a new match
can present again.

## Local verification

The focused browser regression is `tests/rendering/strategic.play.spec.ts`.
It exercises identity, private-report filtering, job aggregation, non-overlapping
labels, resource pixels, historical contacts, reset, layer controls and real
General selection/march/return to sprite zoom on renderer projects.

`tests/rendering/strategic-capture.play.spec.ts` is an opt-in original-art proof
for this checkout's dev server on port 8153. The ignored file
`.cache/showcase/zoom.json` supplies `{"url":"http://127.0.0.1:8153/","mode":"general"}`;
`steward` captures the economy demo and `large` uses the locally prepared
8,000-tile Paris package named in `tests/proofs/strategic/large.ts`, with 99 real
AI opponents. Packages must be installed before starting the dev server. The
large proof stops its AI hosts in its cleanup path. Videos and source-backed
game assets remain outside Git. See the [local evidence record](scale/far-zoom-evidence.md) for actual
captures, measurements and limits.

The cockpit3 source-backed take uses `.cache/showcase/cockpit3.json` with
`{"url":"http://127.0.0.1:8155/","phase":"after"}` (or `before`). It records
original art on the 8,000 × 8,000 Paris package without socket, map, actor or art
mocks. Fog is explicitly disabled for this landscape inspection. Its moving
camera stays within the land during frame sampling. This new-game scene has
four real starting units and one Town Center, not a crowded RTS qualification.
