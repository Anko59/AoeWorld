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
