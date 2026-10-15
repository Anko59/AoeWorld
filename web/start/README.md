# Entry sequence

The root page opens the cinematic once per browser session, then the main menu.
`?skipIntro=1` opens the menu immediately; `?play=1` opens the game directly.
Existing camera fragments also open the game directly, preserving capture links.
`?introStills=1` keeps the cinematic timing but uses its animated poster fallback.

The opening preloads four muted, inline, non-looping clips. Each plays at normal
speed; its final frame holds through the short crossfade. Posters remain beneath
video until playback begins, and handle rejected playback or media errors.
The castle closes the sequence, with the logo revealed after its motion ends.

Skirmish owns map creation using the current map request schema and retained
job driver in `web/map-jobs.js`. Automatic preparation and rivers are defaults.
There is no in-game creator. Quit to Main Menu returns to the entry menu.
The Resume action adopts a retained accepted job into skirmish loading,
including when the driver is already polling it after a reload. Unaccepted
saved submissions, refused jobs and unknown retained IDs are dropped.
Loading Cancel uses the cancellation API and returns to setup after confirmation.
Progress is measured within the current job phase, never an invented aggregate.
Unknown phase counts are indeterminate. After activation, loading waits for the
activated map hash, zero pending and failed scene chunks, a new presented frame,
a ready minimap raster and a connected game. It centres the Town Center
(or the starting scout when no village exists) before fading, including when the geographic
centre is water. A bounded terrain wait reports an error with a route back to setup.

The eight player colours are saved in session storage and passed to the game
as `#playground data-player-colour` and the `--player-colour` CSS property.
The own-unit and building minimap markers adopt that colour. Imported original
unit and building sprite masks currently use blue.

Artwork provenance is in `web/art/README.md`. The bundled Cinzel display font
is from Google Fonts, under `FONT-LICENSE.txt`; screens need no external font
request. Reduced-motion preferences disable pans, fades and bar animation.

Browser specs in `browser/tests/start/` cover entry, requests, progress,
errors, capture hooks and both requested desktop sizes. Their screenshots
are written to `.cache/start-shots/`.

Map size is a fixed choice rather than a free number. At the default 30:1
compression, Large covers 480 km of real landscape (16 km / 8,000 tiles in game);
Giant covers 1,920 km (64 km / 32,000 tiles). The estimate reflects custom
compression. Tiny retains the 30 km preset used for geographic regressions.

On load, a saved submission is resent once after the controller connects, using
its original idempotency key and body. Acceptance is adopted for Start/Resume;
only definitive refusals clear the request. Network failures and server errors
retain it. Recovery has a bounded timeout, so setup can always offer a retry.
The source qualification still checks both preparation profiles through setup
and resumes completed retained jobs after a worker-free server restart.
