# Showcase videos

A pull request above `low` may carry a showcase video of in-game footage that
shows the change in its "What" section; text cards only frame the footage and
never replace it ([shipping](shipping.md)). `make showcase` records it from
a storyboard the agent writes; `make ship SHIP_VIDEO=…` then uploads it.

```sh
make showcase SHOWCASE_STORYBOARD=.cache/showcase/storyboard.json   # → .cache/showcase/showcase.webm
make ship … SHIP_VIDEO=.cache/showcase/showcase.webm
```

`make showcase-check` validates the storyboard, output path and natural
five-minute limit without building either Docker image. `SHOWCASE_LEVEL` sets
the pull request level for the check; it defaults to `SHIP_LEVEL`, then the
level in `.github/pull_request_template.md`. The accepted values are `low`,
`medium`, `high` and `max`. High and max levels require at least one narrated
scene, which the check rejects before any build or speech request. `make showcase` voices
the scenes first, checks the narration-stretched plan against five minutes,
then builds the recording images, prepares dependencies and records the take.

Keep the storyboard outside the repository (or under `.cache/`): it is not
committed. `SHOWCASE_OUT` may select a path inside `.cache/showcase/`; its
filename must use only letters, digits, `.`, `_` and `-`, so `make ship`
accepts it for attachment. Paths outside it, `..` components, and any
symlinked path component are refused.
Existing outputs must be regular files. The directory is created when needed.
Docker binds support `:` in checkout and output parent paths; CSV quoting also
preserves commas and quotes in recorder paths.

## Storyboard

```json
{
  "title": "make ship",
  "voice": "Kore",
  "scenes": [
    {"kind": "card", "heading": "🚢 make ship", "lines": ["before vs after"],
     "narration": "Agents could push unchecked code. Not any more."},
    {"kind": "terminal", "tag": "judge from dev", "tone": "before",
     "caption": "An agent pushes directly",
     "lines": [{"style": "cmd", "text": "git push"}, {"style": "good", "text": "  ✓ allowed"}]},
    {"kind": "browser", "path": "/", "caption": "The lab after the change", "seconds": 4,
     "steps": [{"click": "text=Start"}, {"text": "villager"}, {"key": "Enter"}, {"wait_ms": 1500}]}
  ]
}
```

- **card:** a title card (`heading`, `lines`).
- **terminal:** a scripted transcript, typed live. `tone` (`before`, `after`,
  `neutral`) tags it; line styles are `cmd` (typed), `out`, `good`, `bad`,
  `dim`. Show real output: paste it from the commands you ran.

- **browser:** the app under development, filmed live. `path` (starting with
  one `/`) opens on the app's origin; `caption` is overlaid, again after every
  navigation; `steps` run in order (`click` a Playwright selector,
  `click_at` a position inside one (`{"selector", "x", "y"}`, CSS pixels from
  its top-left; add `"button": "right"` for a context click), turn the mouse `wheel` over such a position
  (`{"selector", "x", "y", "delta_y"}`, up to 2000 px a step, negative is
  away from you), `select` an option value in a `<select>`, press a
  `key`, type `text` at 22 ms a character, or `wait_ms` up to 60 000), then the
  page is held for `seconds` (1–300) after the last step, however long the
  steps took. At most 64 steps. Start the app first (`make dev`).
  `"build": "before"` films the app as it was before the change instead
  (below); `"after"`, or no `build`, is the pull request's app.
- **timing:** the same command before and after as two bars racing
  (`label`, `command` ≤ 200 characters, `before_ms`, `after_ms` of 1 to
  3 600 000, optional `unit` `ms` or `s` and up to 4 `extra` figures
  `{"label", "before", "after"}` of ≤ 40 characters), ending on the ratio
  ("2.4× faster"). Drawn, not run; for performance PRs.
- **api:** a request and its response before and after, as two panes with the
  changed lines highlighted: `caption`, `method`, `path` (starting with `/`),
  optional `request` body, and `before` / `after` as `{"status", "body"}`
  (≤ 22 lines of ≤ 80 characters, ≤ 2000 characters). For backend and
  protocol PRs.
- **agent-session:** a transcript typed out, tool calls and hook denials
  styled apart: `tag`, `caption` and 1 to 40 `steps` of
  `{"role": "user"|"agent"|"tool"|"hook", "text", "verdict"?: "allowed"|"denied"}`
  (text ≤ 600 characters, ≤ 6000 in all; only tool and hook steps carry a
  verdict). `make showcase-session SESSION=<jsonl>` prints these steps from a
  `claude -p --output-format stream-json` or `codex exec --json` log
  (read-only, bounded, secret-looking strings redacted); paste the output
  into `steps`.
- **side by side:** a terminal scene takes `"compare": {"before": [lines],
  "after": [lines]}` (1 to 30 lines of ≤ 100 characters each) instead of
  `lines` and shows two panes, before left and after right, playing at once.

```json
{"kind": "terminal", "tag": "gate", "caption": "Push to dev", "compare": {
  "before": [{"style": "cmd", "text": "git push origin dev"}, {"style": "bad", "text": "  pushed unchecked"}],
  "after": [{"style": "cmd", "text": "git push origin dev"}, {"style": "good", "text": "  ✗ refused: use make ship"}]}},
{"kind": "timing", "label": "Chunk meshing", "command": "make perf-smoke", "before_ms": 2400, "after_ms": 1000,
 "extra": [{"label": "Peak memory", "before": "310 MB", "after": "180 MB"}]},
{"kind": "api", "caption": "Spawn answers 201", "method": "POST", "path": "/api/units",
 "request": "{\"kind\": \"villager\"}",
 "before": {"status": 200, "body": "{\"id\": 7}"},
 "after": {"status": 201, "body": "{\"id\": 7,\n \"owner\": 1}"}},
{"kind": "agent-session", "tag": "hooks", "caption": "A denied push", "steps": [
  {"role": "user", "text": "Push this to dev"},
  {"role": "tool", "text": "git push origin dev", "verdict": "denied"},
  {"role": "hook", "text": "Denied: use make ship", "verdict": "denied"}]}
```

Limit: side-by-side composition is for rendered scenes only. Two browser scenes
of a `before`/`after` pair stay consecutive, not composed into one frame: each
build is reached through its own bridged origin and the recorder drives a
single page, and a second live browser context would share one video and the
origin filter per scene (above), so it is not done.

Set `"renderer": "webgpu"` at the top of the storyboard to film browser
scenes in Chromium with the same software WebGPU flags as the `webgpu`
browser-test project; the default is the plain pinned Chromium (WebGL2).
Set `"viewport": "large"` to film a 1920×1080 page, scaled into the same
1280×720 video, when the change only shows in a desktop-sized window.

The app's origin is `SHOWCASE_APP_URL`, default `http://127.0.0.1:8080/` (the
`make dev` address). It must be plain `http` on `127.0.0.1` or `localhost`
with an explicit port and no credentials, path, query or fragment;
`make showcase-check` refuses anything else when the storyboard has a browser
scene.

## Before and after

A showcase shows the pull request's own change, so film the same path and
steps on both builds. `before` scenes open on `SHOWCASE_BEFORE_URL`, which has
no default, follows the rules of `SHOWCASE_APP_URL` and must use another
port; `make showcase-check` refuses a `before` scene without it.
When it is unset, the shared [dev server](dev-server.md) (`make dev-server`) is the
build before if it is healthy at the revision `origin/dev` is at; otherwise the
error says why. When a
storyboard films both builds, each browser scene is tagged `BEFORE (dev)` or
`AFTER (this PR)` in its top-left corner.

```sh
DEV_PORT=8082 make dev    # in a worktree at origin/dev: the build before
make dev                  # in the pull request's worktree: port 8080
SHOWCASE_BEFORE_URL=http://127.0.0.1:8082/ make showcase SHOWCASE_STORYBOARD=.cache/showcase/storyboard.json
```

```json
{"kind": "browser", "build": "before", "path": "/", "caption": "Seams between chunks", "seconds": 4},
{"kind": "browser", "build": "after", "path": "/", "caption": "One continuous floor", "seconds": 4}
```

`DEV_PORT` is described in [local development](local-development.md); run
`make down` in each worktree afterwards.

## Step library

Fog is enabled in ordinary games. For terrain/art showcases that need the whole
map, start the filmed server with `AOE_FOG_DISABLED=1 make dev DEV_PORT=8097`.
This is a server scenario setting; camera links cannot reveal hidden entities.
Compare's owned spectator servers explicitly disable fog for both sides.
`AOE_FOG_SHOWCASE=1` instead leaves fog enabled and adds an idle wolf and a
distant enemy Town Center beyond the scout's starting LOS, using bounded legal
placement. This supports scouting and remembered DAT building captures; ordinary
games retain their starting kit. The local `building-proof` tool's `fog` mode
scouts both sides of that footprint and returns home through normal move orders.

Browser steps can also be named game steps. Each drives the real client through
the hooks its browser specs use (`web/start`, `H` and `#selection`, the
camera fragment, `data-chunks-pending`), waits for its own readiness, and on a
timeout fails the take with a message naming the step, like
``showcase step 2 `select_entity {"kind":"villager"}`: timed out after 30012 ms
(limit 30000 ms) waiting for the selection panel to show the selection``. Every step takes an
optional `timeout_ms` (default 30 000; `start_skirmish` 180 000; at most 600 000)
that bounds each wait inside it. Waits are measured, not planned, like page loads.

- `{"start_skirmish": {"preset": "alps", "colour"?, "size"?}}`: Single Player,
  the preset (`paris`, `alps`, `brittany`, `finland`), an optional player colour
  and map size (`tiny`, `small`, `large`, `giant`; `tiny` prepares fastest),
  Start Game, then waits for the match on screen, connected, the village HUD up, no chunk pending,
  the minimap raster ready and the frame settled. The scene's `path` must be
  `/?skipIntro=1…`. The setup form has no seed, so there is no `seed`.
- `{"select_entity": {"kind": "villager", "nth"?: 0}}`: presses `H` (centres the
  village) and Escape, then clicks the nth actor of that kind in `#scene
  data-actors` that is on screen (`villager`, `building` = the Town Center,
  `unit` = the scout) and waits for the selection panel. (Not `select`, which
  picks a `<select>` option.)
- `{"command": {"action": "gather", "resource": "wood"}}`: orders the selection:
  frames the nearest resident node of that resource (`data-resource-targets`),
  right-clicks it, and waits for `#workers-wood` to count a villager.
  `{"action": "move", "dx"?: 128, "dy"?: 64}` right-clicks that many canvas
  pixels from the centre. `build` is refused: the client has no build command.
- `{"camera": {"bookmark": "finland-start"}}` or `{"camera": {"x", "y", "zoom"}}`:
  the capture fragment, then waits for the camera read back from `#minimap`, no
  chunk pending and the frame quiet: the settle rule of `make compare`. A
  bookmark's coordinates come from `qa/bookmarks.json`; its renderer is ignored.
- `{"wait_for": {"selector": "#minimap"}}` (visible) or `{"wait_for":
  {"state": "connected|game|chunks_ready|settled|selected"}}`.
- `{"open_panel": {"name": "overview|grid"}}`: toggles it open unless
  it already is, and waits for it.

Refusals happen in `make showcase-check`, before any image is built.

A step that fails says where the page was: the message ends with its state
(`[page: start_state="loading" loading_phase="Sampling overview · 347 / 1,388
pages" raster="pending" …]`) and the latest console errors and failed
requests. While a step waits, the recorder reads that state every second and
prints each change as `showcase-phase page_state`, and a wait slower than a
second as `showcase-phase op_ms`. A page that can no longer succeed fails the
step at once instead of at its limit: the client did not start (no asset
pack, `#unsupported`) or the loading screen shows its error. `gather` frames the node in the
middle of the canvas and clicks where the client reports it, once it is on
screen, clear of the HUD and still; it never expects a computed pixel.
Without a prepared map (`make dev`, no `AOE_PRESET_MAPS`) `start_skirmish`
prepares the real map: about two minutes for `tiny` on a loaded machine.

Before a take with `start_skirmish` the harness makes each map it opens ready on
its server: it requests the minimap raster of every completed job for that
preset and size (`showcase-phase map_warm_ms`). The server computes the raster on
its first request under a 30 s budget and answers 503 past it, and the client
takes that for a failed raster for good, so a start on a freshly restarted or
loaded server used to sit on "Opening terrain" until its limit (21.7 s measured
at load 8 on a restart; 2 ms afterwards). A client whose connection label reads
`server error: …` while loading (a rejected subscription; only a new Welcome
clears it) has the game menu's Reconnect pressed after 4 s, up to three times.

```json
{"title": "Gather", "scenes": [{"kind": "browser", "path": "/?skipIntro=1", "caption": "A skirmish",
  "seconds": 3, "steps": [
  {"start_skirmish": {"preset": "finland", "size": "tiny"}},
  {"select_entity": {"kind": "villager"}},
  {"command": {"action": "gather", "resource": "wood"}},
  {"camera": {"x": 250, "y": 250, "zoom": 2}}]}]}
```

Implementation: `showcase/steps/` expands each step into ops (`script.rs`),
which `steps.mjs` runs; its settle rule is the shared `compare/capture/readiness.mjs`.

## Play capture hooks

Play camera links use `/#x=320.5&y=350.25&zoom=1.75`, on load and
`hashchange`. Each finite value applies
independently, with the normal world and zoom clamps; panning never rewrites
the fragment. Optional `renderer=webgpu|webgl2|canvas` chooses the initial
backend, retaining capability fallbacks; later renderer changes are ignored.
`#minimap` exposes `data-camera-x`, `data-camera-y` and `data-zoom` together.
Its `data-raster` describes the raster for its current `data-map-hash`: `pending`
before or during the request, `ready` after decoding and loading the drawn terrain
image, or `failed` after a request, decode or drawing failure (schematic shown).
Changing maps resets it to `pending`; only state changes write the attribute.
`#scene` exposes `data-chunks-pending`, `data-chunks-failed` and `data-frame`:
pending includes queued visible demand and retries, so zero means visible
terrain has loaded; failed counts visible failures until success or panning
away. Counts change only when needed; the frame counter advances only on a
successful presentation. While height metadata loads, pending stays nonzero.
`AOE_OPEN_MAP_PACKAGE=<content hash> make dev` opens a package from
`local-assets/map-packages` before listening, using the controller's open-map
operation. Unset preserves the default world; an invalid or unknown hash
refuses startup with one plain error naming the hash and package directory.
Terrain reads and camera navigation need no token.

### Bookmarked browser scenes

A browser scene may name a compare bookmark instead of a path:

```json
{"kind":"browser","build":"after","bookmark":"finland-conifer-forest-webgl2",
 "caption":"Finland forest","seconds":2}
```

The name comes from `COMPARE_BOOKMARKS` (default `qa/bookmarks.json`). The map
request resolves against `COMPARE_BEFORE_PACKAGES` / `COMPARE_AFTER_PACKAGES`
(default `local-assets/map-packages`), using compare's exact fragment and map
resolution. Unknown names and ambiguous packages fail `showcase-check` before
speech or image builds. A bookmark permits only an omitted path or `/`, so a
second camera or another page cannot silently override it. WebGPU bookmarks
select the software WebGPU launch; WebGL2 camera pages hide `navigator.gpu`.

Start each filmed build once with its resolved `AOE_OPEN_MAP_PACKAGE` and
`AOE_ASSET_PACK`, as for compare, then reuse its origin across scenes and takes.
Showcase does not build or restart the game server per scene. One bridge per
filmed origin and one browser context serve the whole take. Consecutive cameras on the same
origin and renderer reuse the page through `hashchange` when their fragments
differ and the previous scene had no steps. Renderer changes, repeated fragments
and scenes following interaction steps reload to initialize the backend and
restore the bookmark camera. Stop the servers
with `make down` when finished.

Bookmarks and direct play camera fragments (`x`, `y` or `zoom`) wait for
compare's readiness hooks and settle rule before running scene steps. This
uses the same polling module as compare: zero pending and failed chunks, a
successfully presented frame, then a quiet window clamped to 1.5–4 seconds
from four times the median frame interval. Persistent chunk failures retain
the 12 second retry allowance, and the view retains a 90 second timeout.
The captured bookmark's map, camera and renderer are checked again in Rust
before publication. Camera placement needs no
minimap click or wheel. Explicit storyboard steps still run.
Ordinary UI paths keep their existing interaction sequence, without a static
camera settle requirement.

Remove fixed setup `wait_ms` steps when using readiness. Explicit waits remain
intentional screen time and are always honored. Scene holds still begin after
the steps; settling does not consume the hold or shorten narration. Phase
lines on stdout/stderr identify page loading, settling, narration, image and
dependency preparation, recording and ffmpeg. See the measured costs and
limits in [showcase speed](showcase-speed.md).

## Network

An explicit HTTP port 80 is accepted and normalized to the default-port URL;
the input must still write `:80`. Implicit ports are refused.

Every take runs with `--network none`: the recorder's container has no
network, so WebRTC/STUN, DNS and every address but the filmed apps reach
nothing. A take with browser scenes gets one way out per build it films: the
app's port and, with `before` scenes, the earlier build's port. On the host,
one bridge (`bridge.rs`) per origin listens on its own socket in the run's
private 0700 work directory (`app.sock` for SHOWCASE_APP_URL, `before.sock`
for SHOWCASE_BEFORE_URL) and forwards each connection to that app's loopback
`<host>:<port>` and nowhere else; inside the container, `record.mjs` listens
on each origin's `127.0.0.1:<port>` and pipes every connection to the matching
socket, so Chromium reaches each app at its usual URL. A build no scene films
is not bridged. Each bridge allows 64 open and 4096 total connections and
stops, closing them all, when the recording ends.

The origin filter inside Chromium stays as a second layer, and it is per
scene: a scene reaches the origin of its own build only, so a `before` scene
cannot load anything from the pull request's app or the reverse. Every HTTP(S)
request (`context.route`) and every WebSocket (`context.routeWebSocket`) must
start with that origin's `http://<host>:<port>/` or `ws://<host>:<port>/`
prefix; all others, including other loopback ports such as
`http://localhost:631/`, other hosts, `https`/`wss` and URLs with credentials,
are aborted, and service workers are blocked; cards and terminals allow only
`about:blank` and `data:`. The rule is `AppOrigin::allows` in `browser.rs`,
which `record.mjs` mirrors; `Origins` in `origins.rs` chooses the origins.
Limit: a filmed app is itself reachable without restriction, so a page can
drive any endpoint that app serves.

A change with nothing to see in the game (harness, performance, protocols, AI
decisions) ships without a video; its description carries the numbers.

## Narration

`high` and `max` videos need a voice-over. Any scene may carry `narration`
(≤ 3000 characters); it is spoken by `google/gemini-3.8-flash-lite-tts` on
OpenRouter with the storyboard's `voice` (Kore, Puck, Charon, …), which answers
raw PCM (24 kHz, 16-bit mono) that the harness converts and measures. The
authorization header is sent to curl over a pipe and is never written to disk;
each response is limited to 32 MiB and error details to 64 KiB.
`OPENROUTER_API_KEY` comes from the environment or, when unset, from the
keyring (`secret-tool lookup service codex-api name OPENROUTER_API_KEY`). Speech
requests use only OpenRouter's fixed endpoint.
A scene lasts at least as long as its voice-over.

Successful speech is cached as private PCM under `.cache/showcase-tts/`, keyed
by the exact text, voice, model and PCM format/rate. Identical scenes and later
takes reuse the audio without another TTS call or credential lookup. A changed
text, voice or model misses the cache. Failed, empty, odd-length or oversized
responses are never published to the cache; hits still count toward duration
and aggregate PCM budgets. Cache files and directories cannot be symlinks.
Delete a cache entry to regenerate it (or the cache directory to clear all
speech); a malformed cached file fails with a regeneration hint.

## How it is made

`crates/harness/src/ship/showcase/`: the storyboard must be a regular file no
larger than 256 KiB and is parsed strictly; each
narration is voiced and measured, then one Playwright take in the pinned
browser image (`record.mjs`) plays every scene for its
planned length and records the actual timings. A browser scene's plan is its
steps (waits and typing, counted once) plus its hold; the hold starts after the
page has loaded and the steps ran, while the recorded scene length includes
the page load, so the voice-over stays aligned with what was on screen. The ship-tools image (`docker/ship-tools.Dockerfile`,
ffmpeg) pads each voice to its scene's measured length and muxes the track into
the WebM. Narration can stretch a scene; plans over 5 minutes are rejected
before image builds. A take whose
measured duration exceeds 5 minutes is rejected. The final WebM is written to
a temporary file beside the destination and atomically renamed into place.
Existing multiply linked output files are refused. `make ship`
then checks the level's limit and the sound track.

Showcase reuses installed browser dependencies when the root manifest, lockfile,
npm's installed lock metadata and every installed package version agree.
Missing packages or drift rerun `make browser-deps`; image builds still use
Docker's layer cache each take. This is dependency-graph reuse, not an integrity
scan of every installed file. `make browser-check` continues to install and
check dependencies through its full path. `make test-showcase` runs focused
Rust regressions, and `make test-showcase-browser` runs injected readiness hooks
in Chromium without an app server or game assets.

## Compare

`make compare` answers "what did this change do to the real map?": it
screenshots the same bookmarked camera on the build before the change and on
the working tree, and counts the scene pixels that differ. It is a viewer, not
a gate: it exits 0 whatever the differences, and fails only when a view could
not be captured or checked.

```sh
make compare COMPARE_VIEWS=finland-start,france-alps-cliffs      # → .cache/compare/<branch>/
make compare COMPARE_BEFORE=http://127.0.0.1:8086/ COMPARE_AFTER=build
```

Each side that is `build` (the default) gets its own server on a free port,
started and stopped by the run, one per map that the selected views show. The
server opens the map's stored package at start (`AOE_OPEN_MAP_PACKAGE`), and
each view is captured by a page that only opens the camera fragment of the
bookmark: it issues no order, no creator panel, no
minimap click and no wheel event. `before` is `git archive` of the merge base
with `origin/dev` (`COMPARE_BASE=<rev>` picks another commit) built in the tool
image under `.cache/compare/` (cached per commit, cold build several minutes);
`after` is the working tree, uncommitted edits included, built into `target/`. When the merge base is
`origin/dev` itself, `before` uses the shared [dev server](dev-server.md)'s build
(and says why when it falls back to its own).
A URL instead of `build` uses an app that is already running (plain `http` on
loopback with a port). It must already have the views' map open, and it serves
one map per run.

`qa/bookmarks.json` holds the views:

```json
{"maps": {"finland": {"center_latitude_e7": 615000000, "center_longitude_e7": 255000000, "requested_side_meters": 30000}},
 "bookmarks": [{"name": "finland-start", "map": "finland", "x": 252, "y": 252, "zoom": 1,
                "renderer": "webgpu", "note": "Start area", "verified": true}]}
```

`maps` names a map by its request, which survives a format change that changes
the package's content hash; each side resolves the name to the stored package
with that request in its package directory (`local-assets/map-packages`, or
`COMPARE_BEFORE_PACKAGES` / `COMPARE_AFTER_PACKAGES`, so a map-crate change
can compare two package formats). If more than one stored package carries the
request, the run refuses and lists them (hash, request, file date): keep one in
that directory, or point the setting at a directory that holds one.

`x`, `y` are tiles, `zoom` is the client's 0.25 to 3, and `renderer` is
`webgpu` (default) or `webgl2`. A webgl2 view runs with `navigator.gpu` removed
(what `forceWebGl` does in the browser tests), so WebGPU is really unavailable.
A bookmark with `"verified": false` is skipped unless `COMPARE_VIEWS` names it.
At most 40 views per run, 200 per file.

For each view the page opens `/#x=…&y=…&zoom=…&renderer=…`, which the client
applies on load and on `hashchange`. The run then checks, and refuses a view
that fails any check, naming the reason:

- the camera read back from `#minimap` (`data-camera-x`, `data-camera-y`,
  `data-zoom`) is the bookmark's, within 0.01 tile and 0.1 % zoom;
- the map read back (`data-map-hash`) is the package asked for, so a server
  that opened another map is refused;
- the renderer read back (`#playground data-renderer`) is the bookmark's, so a
  view that ran another backend fails, naming both;
- the view settled (the rule agreed in issue #360): `#scene`
  `data-chunks-pending` is 0 and `data-frame` has stayed unchanged for the
  window of polling (every 250 ms). The window is 1.5 s, or four times the
  median of the last five frame times when that is longer, capped at 4 s
  (`window_ms` in `report.json`). A frame time is an interval between two
  frame changes of at most 1 s; a longer gap is idle (waiting for chunks, or a
  late redraw) and is not counted. A quiet view that was already drawn settles
  when the window closes; a frame or pending change restarts it. A chunk that
  failed and loaded on retry is not a failure (`recovered_chunk_failures` in
  `report.json` counts the drops). A view fails, naming the count, when every
  chunk still missing has failed (`data-chunks-failed` equals
  `data-chunks-pending`, above 0) for 12 s, two of the client's 5 s retries.
  A view that neither settles nor fails within `COMPARE_VIEW_TIMEOUT_SECONDS`
  (default 90) fails too.

The viewport is 1280×720 at device scale 1. If the two sides end on different
cameras the view is "camera mismatch", not a pixel difference. The count covers
the scene (the canvas) only, with every HUD rectangle that overlaps it masked;
pixels that differ under a mask are in `report.json` (`masked_changed_pixels`,
`only_masked_differ`), never in the count.

`COMPARE_OUT` (default `.cache/compare/<branch>/`, always below `.cache/`) gets,
per view, `<name>.before.png`, `<name>.after.png`, `<name>.diff.png` (changed
pixels red, differences under the HUD blue), `<name>.montage.png` (before |
after | diff at 1:1) and, for all views, `montage.png` (one row per view, shrunk
by a whole factor only if it would pass 16 384 px) and `report.json` with the
cameras read back, the renderer and map of each side, settle time and counts.
Files are owned by you. Only the main session and role-less agents run it.

Limits: the packages must be in the current format (`make map-generate` makes
them); the run does not generate or repair them. `COMPARE_BUDGET_MINUTES`
(default 40) bounds the whole run.
