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

Set `"renderer": "webgpu"` at the top of the storyboard to film browser
scenes in Chromium with the same software WebGPU flags as the `webgpu`
browser-test project; the default is the plain pinned Chromium (WebGL2).

The app's origin is `SHOWCASE_APP_URL`, default `http://127.0.0.1:8080/` (the
`make dev` address). It must be plain `http` on `127.0.0.1` or `localhost`
with an explicit port and no credentials, path, query or fragment;
`make showcase-check` refuses anything else when the storyboard has a browser
scene.

## Network

Every take runs with `--network none`: the recorder's container has no
network, so WebRTC/STUN, DNS and every address but one reach nothing. A take
with a browser scene gets exactly one way out, the app's port. On the host,
the harness's bridge (`bridge.rs`) listens on `app.sock` in the run's private
0700 work directory and forwards each connection to the app's loopback
`<host>:<port>` (SHOWCASE_APP_URL) and nowhere else; inside the container,
`record.mjs` listens on `127.0.0.1:<port>` and pipes every connection to that
socket, so Chromium reaches the app at its usual URL. The bridge allows 64
open and 4096 total connections and stops, closing them all, when the
recording ends.

The origin filter inside Chromium stays as a second layer. Every HTTP(S)
request (`context.route`) and every WebSocket (`context.routeWebSocket`) must
start with the app's `http://<host>:<port>/` or `ws://<host>:<port>/` prefix;
all others, including other loopback ports such as `http://localhost:631/`,
other hosts, `https`/`wss` and URLs with credentials, are aborted, and service
workers are blocked; cards and terminals allow only `about:blank` and `data:`.
The rule is `AppOrigin::allows` in `browser.rs`, which `record.mjs` mirrors.
Limit: the app itself is reachable without restriction, so a page can drive
any endpoint the app serves.

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
