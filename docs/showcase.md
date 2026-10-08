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
     "lines": [{"style": "cmd", "text": "git push"}, {"style": "good", "text": "  ✓ allowed"}]}
  ]
}
```

- **card:** a title card (`heading`, `lines`).
- **terminal:** a scripted transcript, typed live. `tone` (`before`, `after`,
  `neutral`) tags it; line styles are `cmd` (typed), `out`, `good`, `bad`,
  `dim`. Show real output: paste it from the commands you ran.

Browser scenes are planned for a follow-up issue. The recorder runs with no
network and aborts every request except `about:blank` and `data:`.

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
browser image (`record.mjs`) plays every card and terminal scene for its
planned length and records the actual timings. The ship-tools image (`docker/ship-tools.Dockerfile`,
ffmpeg) pads each voice to its scene's measured length and muxes the track into
the WebM. Narration can stretch a scene; plans over 5 minutes are rejected
before image builds. A take whose
measured duration exceeds 5 minutes is rejected. The final WebM is written to
a temporary file beside the destination and atomically renamed into place.
Existing multiply linked output files are refused. `make ship`
then checks the level's limit and the sound track.
