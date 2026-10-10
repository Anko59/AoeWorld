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
  its top-left), `select` an option value in a `<select>`, press a
  `key`, type `text` at 22 ms a character, or `wait_ms` up to 60 000), then the
  page is held for `seconds` (1–300) after the last step, however long the
  steps took. At most 64 steps. Start the app first (`make dev`).
  `"build": "before"` films the app as it was before the change instead
  (below); `"after"`, or no `build`, is the pull request's app.

Set `"renderer": "webgpu"` at the top of the storyboard to film browser
scenes in Chromium with the same software WebGPU flags as the `webgpu`
browser-test project; the default is the plain pinned Chromium (WebGL2).

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
