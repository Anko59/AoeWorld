# Entry sequence

The root page opens the cinematic once per browser session, then the main menu.
`?skipIntro=1` opens the menu immediately; `?play=1` opens the game directly.
Existing camera fragments also open the game directly, preserving capture links.
`?introStills=1` keeps the cinematic timing but uses its animated poster fallback.
The game renderer starts after the cinematic finishes or is skipped, so GPU
recovery cannot replace the entry document during its playback.

The opening preloads four muted, inline, non-looping clips. Each plays at normal
speed. A shot follows its clip's own length: the next shot begins its 0.7 s
crossfade while the previous clip plays its last 0.7 s, so no final frame is
held (`shotSchedule` in `cinematic.js`; clips default to 3 s until their
metadata loads). Posters remain beneath video until playback begins, and handle
rejected playback or media errors.
The castle closes the sequence, with the logo revealed after its motion ends.

Skirmish owns map creation using the current map request schema and retained
job driver in `web/map-jobs.js`. Automatic preparation and rivers are defaults.
There is no in-game creator. Quit to Main Menu returns to the entry menu.
The Resume action adopts a retained accepted job into skirmish loading,
including when the driver is already polling it after a reload. Unaccepted
saved submissions, refused jobs and unknown retained IDs are dropped.
Loading Cancel uses the cancellation API and returns to setup after confirmation.
Loading shows one monotonic bar from submission until the game opens
(`progress-model.js`, drawn by `loading-bar.js`). Stages run in order (job
phases, activation, match build, terrain wait); each has an expected duration
scaled by map size (area) and opponents, and calibrated by the median ratio of
this browser's last six runs (`aoeworld.load-history` in local storage). The bar
is elapsed / (elapsed + estimated remaining), eased every frame and clamped so
it never goes backwards and never stops; only the real finish reaches 100%. Server counts
(pages, villages, chunks) refine the current stage and extrapolate between polls.
The remaining-time estimate rescales the unfinished stages by this run's observed
speed and blends toward each new estimate, so it converges. A stage the server
skips (a stored map) leaves the bar's total. The server's text and counts stay in
the status line; `aoeStart.loadingReport()` returns the last run's samples.
After activation, loading waits for the
activated map hash, zero pending and failed scene chunks, a new presented frame,
and a connected game. The minimap may still be loading or using its schematic;
its background request and retries continue during play. It centres the Town Center
(or the starting scout when no village exists) before fading, including when the geographic
centre is water. Preparation and activation failures offer a route back to setup.

The eight player colours are saved in session storage and passed to the game
as `#playground data-player-colour` and the `--player-colour` CSS property.
Units, buildings, hidden-unit contours and minimap markers use their owner's
colour on WebGPU, WebGL2 and Canvas. Players 1–8 use blue, red, green, yellow,
cyan, purple, grey and orange. Choosing a banner swaps that palette slot with
the human's slot. Beyond eight, the palette cycles with alternating darker
(9% per step) and lighter (7% per step) shades, repeating after 104 players;
the supported 100-player match has 100 distinct RGB colours. Recolouring uses
the imported player masks, preserving unmasked artwork and shadows.
Selecting a foreign actor shows its player number and hides commands. Hostile
selections have red rings; units grant vision and accept orders only for their
owner.

**Local paintings.** `make assets-series-art` writes JPEG copies of the owner's
24 generated series and `manifest.json` into ignored `local-assets/series-art/`
(never Git); the server publishes them at `/series-art/` when that directory
exists (`art.js`). The main menu shows a random "Kings with globes" painting on
every visit. Each loading screen picks a random series and shows its paintings
one after another, 5 s each with a crossfade and slow drift (`loading-art.js`),
named in a small caption. Without the copies the bundled paintings remain.

Artwork provenance is in `web/art/README.md`. The bundled Cinzel display font
is from Google Fonts, under `FONT-LICENSE.txt`; screens need no external font
request. Reduced-motion preferences disable pans, fades and bar animation.

Browser specs in `browser/tests/start/` cover entry, requests, progress,
errors, reload resume, capture hooks and both requested desktop sizes. Their screenshots
are written to `.cache/start-shots/`.

The opt-in `skirmish-startup-*` phase of `make ui-proof` measures a real-art
France start with 99 opponents, at 960 km and 240:1 compression. Point
`.cache/ui/proof.json` at an idle, real-pack development server, for example
`{"phase":"skirmish-startup-run","url":"http://127.0.0.1:8109/"}`.
Prepare that source-backed request with `AOE_PRESET_MAPS` before measuring.
Restart the server and use a fresh browser context for each cold minimap run.
Results under `.cache/startup/` separate the playable frame from the optional
raster's completion and retain resource timings, status phases and screenshots.

**Opponents** sets 0 to 99 AI players (default 7, saved in session storage),
each with the standard start and uncapped population. A server
without an AI host (`AOE_AI_COMMAND`; `make dev` sets it) disables the
setting, and the game is played alone. With opponents, once the map job has
completed `web/start/skirmish.js` asks the server for the match
(`POST /skirmish`) instead of activating the map, and loading shows the
server's own phases with their counts: opening the prepared map, villages
placed n/N, AI players seated n/M. Only then does the usual terrain wait
start. Cancel and Quit to Main Menu call `POST /skirmish/stop`, which ends
the AI processes; a new game or any other map replaces them too.

**Returning resumes the match.** A tab holding a resume token (local
storage, shared by tabs on this origin) first asks `GET /skirmish` before choosing
the cinematic or the menu. When the server reports phase `ready`, the page skips
both and opens the loading screen ("Returning to your game") on the usual
terrain wait for that `content_hash`, then centres the Town Center. The game's
socket sends the same token, so the server gives the tab its human seat back,
whether or not another page sits in it. An unrelated world's token is replaced
by a fresh one for this world. The AI players keep playing
throughout: the server stops them only after 5 minutes with nobody in the human
seat. If no match is running, the page falls back to the cinematic or menu as usual.

The terrain wait has no wall-clock failure deadline: a ready, connected match
keeps loading under CPU pressure. The scene's pending chunk count is shown as
"Opening terrain · n chunks remaining"; retrying terrain requests have their
own message. Definitive preparation/activation errors still open the
error screen. Both new games and reloads use this wait.

The **France** preset is all of mainland France (46.6 N, 2.5 E) at the
Country size, 960 km at 30:1 (16,000 tiles a side): the map of
`make scale-match`. Its map job reuses the source-backed package when the
server holds it and was started with that request in `AOE_PRESET_MAPS`;
otherwise the server prepares it like any other request.

Map size is a fixed choice rather than a free number. At the default 30:1
compression, Large covers 480 km of real landscape (16 km / 8,000 tiles in game);
Country covers 960 km (32 km / 16,000 tiles); Giant covers 1,920 km (64 km / 32,000 tiles). The estimate reflects custom
compression. Tiny retains the 30 km preset used for geographic regressions.

On load, a saved submission is resent once after the controller connects, using
its original idempotency key and body. Acceptance is adopted for Start/Resume;
only definitive refusals clear the request. Network failures and server errors
retain it. Recovery has a bounded timeout, so setup can always offer a retry.
The source qualification still checks both preparation profiles through setup
and resumes completed retained jobs after a worker-free server restart.
