# Dark Age unit and wildlife art

The private VER 5.7 dat table is authoritative. `make assets-dat-table` follows
each unit's dead-unit reference to its decay graphic; this is not an adjacent
SLP-id inference. The catalogue contains identifiers and timing, never pixels
or a copy of the decoded game data.

| Actor | Stand | Walk | Attack | Die | Decay |
| --- | ---: | ---: | ---: | ---: | ---: |
| Militia | 993 | 997 | 987 | 990 | 994 |
| Male villager | 1479 | 1484 | 1473 | 1476 | 1481 |
| Male hunter | 1525 | 1531 | 1518 | 1522 | 1527 |
| Male shepherd | 3684 | 3686 | 3682 | 3683 | 4649 |
| Sheep | 3629 | 3634 | — | 3626 | 3631 |
| Deer | 342 | 348 | — | 339 | 344 |
| Boar | 2557 | 2559 | 2555 | 2556 | 2558 |
| Wolf | 1635 | 1640; run 1636 | 1629 | 1632 | 1637 |

Hunters and shepherds butcher with 1528 and carry food with 1519. The wire
retains their profession at death. Scout combat uses 2079/2082/2086;
its existing standing/walking catalogue remains separate.

Five native rows supply eight facings: rows 0,1,2,3,4,3,2,1, mirroring facings
1–3. Anchors remain signed, including out-of-frame hotspots. The selected frame
is packed above the low three facing bits in the scene DTO. Source-specific
frame durations drive living animation; death is a clamped one-shot using the
wire death tick, followed by five six-second decay frames. Unit removal remains
authoritative. Animal food depletion controls final carcass removal.

Each source is optional only when absent. A present incomplete or duplicate
prefix is a pack error. Missing sources use the existing semantic marker;
there is no unrelated sprite substitution. Every listed source is present in
the local trial, so no DE unit import is needed. Sheep do not attack in this
simulation; the absent trial sheep attack 3623 is not selected.

The actor selection has 2,540 frames, alongside the base catalogue's 2,369
full-prefix frames and the bounded building subset. The runtime validates at
most 5,120 selected frames before pixel allocation. Terrain keeps pages 0–1;
objects use pages 2–3 of four 2048-square RGBA pages (64 MiB). All backends
account for the fourth page. Source PNGs are decoded one at a time.
Sheep have player-mask pixels: owned sheep use the selected player ramp;
neutral sheep get separate grey-ramp copies of their four animation groups.

The existing cursor importer has no native game cursor export. Attack and hunt
therefore use the browser crosshair fallback, with distinct semantic cursor
states. HP and combat statistics come from authoritative HP and the current
simulation combat table; selected living actors receive health bars.
