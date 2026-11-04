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
| F1 | Ownership | Occupied settlement cells, not invented territorial borders |
| F2 | Armies | Army tokens and unit clusters |
| F3 | Economy | Vassal cards and their Castle domains |
| F4 | Threats | Historical enemy sightings, labelled with age, for two minutes |
| F5 | Resources | Discovered deposits, retained across camera subscriptions and terrain eviction |
| F6 | Roads & bridges | Known bridges; road-network data is unavailable |

Vassal cards select the same Castle as near zoom. They show the server's
managed/resident counts and growth wait reason, plus observed food, wood, gold,
stone jobs and idle workers identified by their authoritative manager. Job
counts describe the current authorized unit stream; they are not an invented
global census. A question mark means the report is unavailable. An ungarrisoned
Vassal's domain is inactive. The Castle selection path is the hook for future
Vassal sliders; this revision has no Vassal settings protocol or sliders.

## Work bounds

The existing server stream bounds unit and building residency and includes own
economy units outside the camera region. Enemy presentation additionally checks
current fog visibility. New summaries are built at the four-Hz HUD cadence, never
by scanning all players on every animation frame. The client forwards numeric
records; JavaScript aggregates worker jobs and order destinations to conserve
WASM space. Territory geometry and unchanged DOM content are cached.

Presentation is capped at 256 General records, 64 Castle records, 256 bridge
records, 1,024 historical contacts and the existing 2,048 deposit markers. Nearby
labels use a bounded screen-grid placement search and retain lines to their true
locations. These bounds are not a qualification of a crowded 1,000-player RTS.
The strategic scene retains one terrain raster and fog plane; zooming does not
request detailed terrain for the whole map.

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

Each grouped order sends up to four nearest suitable villagers. Free idle
workers take priority, then other free workers; managed workers can answer
when no free workers are available. Workers already building, fighting or
hunting are excluded. Ties use entity ID. No selection or zoom change is needed.
Feedback says Submitting until the ordinary server acknowledgement arrives.
One build can be pending at a time. The client tracks outgoing commands against
the human burst of 20 and refill of 10 per second; the server retains authority.
Esc, right-click, focus loss and returning to near zoom cancel placement.

## First far view

The first far zoom in each match opens a short parchment introduction to armies
and kingdoms, Units here and building, and jump keys. Next advances immediately;
Skip tour, Esc, clicking into the game or returning to near zoom dismisses it.
It highlights the relevant cockpit controls in three brief steps and respects
reduced-motion preferences. The game and all commands remain live throughout.
A session-stored digest of the match resume identity prevents replays on reload
or reconnect; a new match can present again. It adds no invented kingdom counts.

## Local verification

The focused browser regression is `tests/rendering/strategic.play.spec.ts`.
It exercises identity, private-report filtering, job aggregation, non-overlapping
labels, resource pixels, historical contacts, reset, layer controls and real
General selection/march/return to sprite zoom on renderer projects.

`tests/rendering/strategic-capture.play.spec.ts` is an opt-in original-art proof
for this checkout's dev server on port 8153. The ignored file
`.cache/showcase/zoom.json` supplies `{"url":"http://127.0.0.1:8153/","mode":"general"}`;
`vassal` captures the economy demo and `large` uses the locally prepared
8,000-tile Paris package named in `tests/proofs/strategic/large.ts`, with 99 real
AI opponents. Packages must be installed before starting the dev server. The
large proof stops its AI hosts in its cleanup path. Videos and source-backed
game assets remain outside Git. See the [local evidence record](scale/far-zoom-evidence.md) for actual
captures, measurements and limits.
