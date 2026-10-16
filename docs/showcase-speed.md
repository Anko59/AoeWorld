# Showcase recording cost — 2026-10-16

Baseline: `7f6b046228167d5a96a78e25fb43fc3755065c6f` (`origin/dev`).
Measurements use one narrated terminal scene and two narrated browser scenes
labelled before/after, on the same baseline app build: the change is in video
tooling, not the game's output. The browser camera is the real
`finland-conifer-forest-webgl2` bookmark (100, 100, zoom 1). Each browser scene
holds for two seconds; the original storyboard waits ten seconds for setup.
Both scenes speak identical text with Kore to expose redundant requests.

## Method

One Make target at a time. Cargo and browser tests run in the pinned Docker
images. Images and Cargo downloads were warm; this was a fresh worktree.
Transparent local wrappers measure Docker/curl subprocess wall time without
logging credentials. The baseline's private recorder copy logs page load,
explicit waits and scene duration without changing its behavior. The revised
recorder logs those phases itself. Logs and storyboards stay in the ignored
`.cache/video-speed/`; phase durations inside the recorder overlap its total.
The branch binary is used to exercise new showcase behavior because
`make showcase` deliberately runs dev's immutable judge until publication.

The package is
`5a63a78c486c202bc3cff4cd86d0f9fbff0252c2a37a7124515a195a9f1ab68c`.
Two pre-started origins serve it throughout the takes. Startup takes 2.621 s
and 2.766 s, separately from recording. The original implementation already
reuses external servers across scenes and takes, with one bridge per filmed
origin. That lifecycle is preserved and regression-tested; no second server
manager or per-scene build is added. Camera changes on the same origin and
renderer now reuse the app document through its fragment too.

## Baseline

Seconds, one observation each. Recorder subphases must not be added again.

| Phase | Terminal | Browser pair |
|---|---:|---:|
| Whole command | 16.110 | 36.480 |
| TTS | 2.625 (1 call) | 3.845 (2 calls) |
| Recording image builds | 0.770 | 0.731 |
| Dependency preparation | 3.149 | 2.231 |
| Recorder total | 7.350 | 26.315 |
| Page loads inside recorder | 0 | 0.467 |
| Fixed setup waits inside recorder | 0 | 20.000 |
| Scenes inside recorder | 5.240 | 24.472 |
| ffmpeg | 1.720 | 2.769 |

Fixed setup waits dominate browser recording (55% of the whole command).
TTS repeats the same paid request and can be eliminated on subsequent takes.
Later observations made repeated dependency installation dominant too; the final
implementation reuses an installed lock graph. ffmpeg still runs each take.

The first `make dev` attempt took 363.32 s: 207 s queued for a heavy slot,
45.97 s compiling WASM, 57.96 s compiling the release server and 44.84 s
compiling the map worker. Startup then failed because the measurement setup
had symlinked the stored-map parent, which storage intentionally refuses.
Using a real directory fixed the setup; no deadline or path check was relaxed.
That failed cold setup is separate from the successful take timings.

## Re-measurement

Final terminal: **16.11 → 8.84 s** (45.1% lower wall time). Browser pair:
**36.48 → 31.57 s** (13.5% lower) after replacing only the two setup waits
with the equivalent bookmark/readiness operation. These are observed commands,
not isolated estimates of causality. The captions, voice text, camera, builds
and requested two-second holds stay the same. The recorder still stretches
holds to cover narration; the optimized pair holds 4.56 s per scene.

The unchanged browser storyboard also ran: **48.13 s**, slower because its
20 s of explicit waits remain on top of readiness. Fixed waits are not
silently skipped. Its first scene made one TTS call; the identical second
scene hit the cache. The final terminal and bookmark pair are warm-cache takes
with zero TTS calls and no npm install.

| Phase (seconds) | Terminal before | Terminal final | Browser before | Browser unchanged | Browser bookmark |
|---|---:|---:|---:|---:|---:|
| Whole command | 16.110 | 8.840 | 36.480 | 48.130 | 31.570 |
| TTS calls (count) | 1 | 0 | 2 | 1 | 0 |
| TTS requests | 2.625 | 0.000 | 3.845 | 1.988 | 0.000 |
| Image builds | 0.770 | 0.624 | 0.731 | 0.660 | 0.647 |
| Dependency install | 3.149 | 0.000 | 2.231 | 0.000 | 0.000 |
| Recorder total | 7.350 | 6.538 | 26.315 | 43.001 | 29.147 |
| Page load (inside recorder) | 0.000 | 0.000 | 0.467 | 0.315 | 0.342 |
| Settle (inside recorder) | 0.000 | 0.000 | 0.000 | 17.124 | 18.248 |
| Explicit waits (inside recorder) | 0.000 | 0.000 | 20.000 | 20.000 | 0.000 |
| Hold (inside recorder) | 3.767 | 3.415 | 4.000 | 4.000 | 9.120 |
| ffmpeg | 1.720 | 1.391 | 2.769 | 2.055 | 1.446 |
| Other orchestration | 0.496 | 0.287 | 0.588 | 0.426 | 0.330 |

Readiness now dominates the browser pair (18.248 s): client startup while
hooks are unavailable, followed by the quiet window. Missing hooks, pending
chunks and an unpresented frame are all rejected. The zero-frame guard is
covered by injected tests; zero pending with zero frames was not observed in
these real takes. The recorded scenes take longer than the originals, while speech and dependency reuse
reduce total command time. No deadline, quiet window or hold was reduced.

An additional real command changed from the forest bookmark to its quarter-
zoom bookmark on one origin: 24.64 s total. Page/hash navigation was 162/84 ms,
settling 8586/1848 ms. Both named cameras passed the Rust checks. The same
server container IDs remained alive through the initial browser takes and compare;
both servers were stopped, waited for and removed afterwards.

Intermediate terminal runs before dependency reuse were slower under changing
load: 36.68 s cold speech and 35.21 s warm speech. They included 11.974/13.961 s
inside dependency preparation and 16.022/11.676 s inside the recorder. These
observations justified reusing installed dependencies and are retained in the
local logs, rather than discarded. No empty-audio-cache terminal run of the
final dependency-reusing implementation was measured.

The branch command `compare` captured the same bookmark through the shared
polling module: WebGL2, matching cameras, identical pixels. `make video-probe`
reported a 27.81 s VP8 1280×720 video with mono Opus audio for the final browser
pair. A separate screenshot montage was visually inspected; full video
playback and listening were not performed.

## Final binary confirmation

After all required checks, the final formatted branch binary repeated the
warm-cache storyboards: **8.95 s terminal** and **29.95 s browser pair**, versus
16.11 s and 36.48 s on dev (44.4% and 17.9% lower observed wall time). The
earlier 8.84/31.57 s observations above are retained to show variability.
Both confirmation takes made zero TTS requests and reused installed dependencies.
A fresh pair of the same baseline servers was started once for this confirmation,
reused by the browser scenes, then stopped and removed.

| Phase (seconds) | Terminal confirmation | Browser confirmation |
|---|---:|---:|
| Whole command | 8.950 | 29.950 |
| TTS | 0.000 | 0.000 |
| Image builds | 0.623 | 0.738 |
| Dependency install | 0.000 | 0.000 |
| Recorder total | 6.498 | 27.666 |
| Page load (inside recorder) | 0.000 | 0.321 |
| Settle (inside recorder) | 0.000 | 16.870 |
| Explicit waits (inside recorder) | 0.000 | 0.000 |
| Hold (inside recorder) | 3.414 | 9.119 |
| ffmpeg | 1.560 | 1.237 |
| Other orchestration | 0.270 | 0.310 |

The confirmation browser artifact has VP8 video and Opus audio; stream
inspection is recorded in the local confirmation log. The measured binary
SHA-256 is `6f6fb0f1a612a78fffabeb52b782dedf611e21148829cb93223c6bf30d571aec`.

## Limits

These are single-host observations under shared load, not isolated benchmark
statistics, cold-image download measurements, RTS performance or dedicated
hardware qualification. TTS request counts measure avoided provider calls;
no dollar or token-cost estimate is claimed. Provider latency and generated
audio duration vary. Warm-cache results apply after a successful first request.
Explicit storyboard waits remain intentional screen time and are not removed
automatically; replacing setup waits is an authoring change.
