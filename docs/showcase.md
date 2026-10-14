# Showcase videos

Every pull request above `low` carries a showcase video that sells the change
in its "What" section ([shipping](shipping.md)). `make showcase` records it from
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
then builds the recording images and records the take.

Keep the storyboard outside the repository (or under `.cache/`): it is not
committed. `SHOWCASE_OUT` may select a path inside `.cache/showcase/`; its
filename must use only letters, digits, `.`, `_` and `-`, so `make ship`
accepts it for attachment. Paths outside it, `..` components, and any
symlinked path component are refused.
Existing outputs must be regular files. The directory is created when needed.

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
port; `make showcase-check` refuses a `before` scene without it. When a
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

## Play capture hooks

Play camera links use `/#x=320.5&y=350.25&zoom=1.75`, on load and
`hashchange`, for controllers and spectators. Each finite value applies
independently, with the normal world and zoom clamps; panning never rewrites
the fragment. Optional `renderer=webgpu|webgl2|canvas` chooses the initial
backend, retaining capability fallbacks; later renderer changes are ignored.
`#minimap` exposes `data-camera-x`, `data-camera-y` and `data-zoom` together.
`#scene` exposes `data-chunks-pending`, `data-chunks-failed` and `data-frame`:
pending includes queued visible demand and retries, so zero means visible
terrain has loaded; failed counts visible failures until success or panning
away. Counts change only when needed; the frame counter advances only on a
successful presentation. While height metadata loads, pending stays nonzero.
`AOE_OPEN_MAP_PACKAGE=<content hash> make dev` opens a package from
`local-assets/map-packages` before listening, using the controller's open-map
operation. Unset preserves the default world; an invalid or unknown hash
refuses startup with one plain error naming the hash and package directory.
Terrain reads and camera navigation need no controller lease.

## Network

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

Not being a frontend change is never a reason for no video: use before/after
terminal scenes for the harness, timings for performance, request/response for
protocols, a filmed agent session for developer experience.

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

## Compare

`make compare` answers "what did this change do to the real map?" without a
second server, a controller lease or a hunt for cameras: it screenshots the
same bookmarked camera on the build before the change and on the working
tree, and counts the pixels that differ. It is a viewer, not a gate: it exits 0
whatever the differences, and fails only when a view could not be captured.

```sh
make compare COMPARE_VIEWS=finland-start,france-alps-cliffs      # → .cache/compare/<branch>/
make compare COMPARE_BEFORE=http://127.0.0.1:8086/ COMPARE_AFTER=build
```

Each side that is `build` (the default) gets its own server on a free port,
started and stopped by the run, with one browser session that is the gameplay
controller of that server alone; so nothing waits for a lease and no other
agent's server is touched. `before` is `git archive` of the merge base with
`origin/dev` (`COMPARE_BASE=<rev>` picks another commit) built in the tool
image under `.cache/compare/` (cached per commit, cold build several minutes);
`after` is the working tree, uncommitted edits included, built into `target/`.
A URL instead of `build` uses an app that is already running (plain `http`
on loopback with a port): the run then takes *that* server's controller lease.

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
can compare two package formats). `x`, `y` are tiles, `zoom` is the client's
0.25 to 3, `renderer` is `webgpu` (default) or `webgl2`. A bookmark with
`"verified": false` is skipped unless `COMPARE_VIEWS` names it. At most 40
views per run, 200 per file.

For each view the run opens the package once per session, zooms with the wheel,
places the camera with minimap clicks aimed again from the camera read back
(`aoeNavigation.frame`), and waits until the `terrain cache: … chunks` count,
the camera and the map requests in flight stay unchanged for five polls
(`COMPARE_VIEW_TIMEOUT_SECONDS`, default 90, bounds it; `COMPARE_BUDGET_MINUTES`,
default 40, bounds the run). The viewport is 1280×720 at device scale 1. If the
two sides end on different cameras the view is "camera mismatch", not a pixel
difference. The count covers the scene (the canvas) only, with every HUD
rectangle that overlaps it masked; pixels that differ under a mask are in
`report.json` (`masked_changed_pixels`, `only_masked_differ`), never in the count.

`COMPARE_OUT` (default `.cache/compare/<branch>/`, always below `.cache/`) gets,
per view, `<name>.before.png`, `<name>.after.png`, `<name>.diff.png` (changed
pixels red, differences under the HUD blue), `<name>.montage.png` (before |
after | diff at 1:1) and, for all views, `montage.png` (one row per view, shrunk
by a whole factor only if it would pass 16 384 px) and `report.json` with the
cameras read back, chunks, settle time and counts. Files are owned by you.
Only the main session and role-less agents run it.

Limits: a spectator cannot open a map and the camera has no URL or setter, so
the harness drives documented input events; a minimap click is only as precise as
the aiming loop (the read-back camera is in the report). Follow-ups: a camera URL
fragment and a "chunks loaded" attribute in the client would replace the two
small objects (`camera`, `settle`) in `capture.mjs`; a storyboard
`"bookmark": "<name>"` field on browser scenes would film the same cameras.

