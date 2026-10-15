# Entry sequence

The root page opens the cinematic once per browser session, then the main menu.
`?skipIntro=1` opens the menu immediately; `?play=1` opens the game directly.
Existing camera fragments also open the game directly, preserving capture links.

Skirmish uses the map creator's request schema and retained job driver in
`web/map-jobs.js`. Automatic source preparation and rivers remain the defaults.
Progress is measured within the current job phase, never an invented aggregate.
Unknown phase counts are indeterminate. After activation, loading waits for the
activated map hash, zero pending and failed scene chunks, a new presented frame,
a ready minimap raster and a connected game. A bounded terrain wait reports an
error with a route back to setup.

The eight player colours are saved in session storage and passed to the game
as `#playground data-player-colour` and the `--player-colour` CSS property.
The primary-unit minimap marker adopts that colour. Unit sprite recolouring is
left to the economy/interface work, since the current client has no player
colour setting.

Artwork provenance is in `web/art/README.md`. The bundled Cinzel display font
is from Google Fonts, under `FONT-LICENSE.txt`; screens need no external font
request. Reduced-motion preferences disable pans, fades and bar animation.

Browser specs in `browser/tests/start/` cover entry, requests, progress,
errors, capture hooks and both requested desktop sizes. Their screenshots
are written to `.cache/start-shots/`.
