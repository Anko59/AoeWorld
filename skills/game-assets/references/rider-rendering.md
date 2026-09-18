# Worked example: the cavalry rider

Read this when wiring imported frames into the game, changing animation,
or debugging appearance. The concrete code is in
`crates/client/src/game_assets.rs`,
`crates/rendering/src/{game_renderer,playground}.rs`, and
`crates/client/src/playground_game.rs`. The example describes the current
Age of Kings trial pack; inspect another pack before copying its numbers.

## Select and compact only the art the game needs

The tested trial pack has 20,396 frames across 27 atlas pages. The playable
client does not upload them all. It selects 50 frames from
`graphics.drs:[32, 112, 108, 115]:3008` (walking cavalry), 50 from `:3004`
(standing cavalry), and the first 10 from
`terrain.drs:[32, 112, 108, 115]:15008` (grass). It confirms the selected
frame numbers are contiguous, copies their pixel rectangles into a single
2048×2048 runtime RGBA atlas, and keeps a `GameFrame` record per frame:

```text
GameFrame {
    uv: [x / 2048.0, y / 2048.0, width / 2048.0, height / 2048.0],
    size: [width, height],
    anchor: [anchor_x, anchor_y],
}
```

The importer and game loader use pixel dimensions; the renderer normalizes
UVs for the shader. Each record's rectangle and anchor can differ across
frames. Do not use one "unit width" or one hotspot for the entire animation.

The current runtime compositing order is: player mask where present,
otherwise palette color, otherwise shadow. The player mask's red channel in
this resource is a shade index `0..7`, *not* a normalized 0–255 intensity.
The current blue player color is `[65, 145, 245]`; its shade is
`0.65 + min(mask_red, 7) / 7 * 0.35`. Player pixels are opaque. Shadow is
copied with its stored alpha when no color or player pixel is present.
The viewer also displays the outline layer; the current game compaction does
not. Its player tint currently uses a different shade divisor, so compare
shape and masks there but judge final player color in the game. If changing
this look, decide the compositing rule explicitly and keep WebGPU and Canvas
2D on the same final atlas pixels.

Upload that atlas once. Both game backends consume the same `GameArt` frame
metadata and texture content; a WebGPU initialization failure switches to a
fresh Canvas 2D canvas. Do not decode/upload an atlas per entity, movement
tick, frame, or viewport change. The world can grow without growing this
fixed local art selection.

## Direction, time, and hotspot

The 50 walking frames are five rows of ten. The simulation's `Facing` values
run south, southeast, east, northeast, north, northwest, west, southwest
as `0..7`. The game maps them to source rows and mirroring as follows:

| Facing | Row | Mirror horizontally |
| --- | ---: | --- |
| South (0) | 0 | No |
| Southeast (1) | 1 | Yes |
| East (2) | 2 | Yes |
| Northeast (3) | 3 | Yes |
| North (4) | 4 | No |
| Northwest (5) | 3 | No |
| West (6) | 2 | No |
| Southwest (7) | 1 | No |

The formula is `frame_index = row * 10 + (moving ? animation % 10 : 0)`.
Standing uses the first frame in its row. The current client calculates
`animation = floor(performance_time_ms / 100)`, so walking advances every
100 ms and loops after one second. This is a visual clock, not simulation
state; gameplay movement comes from the server's world position/facing.

For a frame of width `w`, height `h`, anchor `(ax, ay)`, zoom `z`, and
projected unit point `(sx, sy)`, the draw rectangle begins at:

```text
unmirrored: x = sx - ax*z;       y = sy - ay*z
mirrored:   x = sx - (w-ax)*z;   y = sy - ay*z
size:       (w*z, h*z)
```

Mirroring also changes UV from `(u, v, du, dv)` to
`(u + du, v, -du, dv)`. In Canvas 2D, translate by the draw width and scale
X by `-1` while sampling the positive source rectangle. Doing only the UV
flip makes the rider jump around its ground point. Doing only the position
change makes it face the wrong way. Preserve the isometric world-to-screen
projection and depth ordering from `game_renderer.rs`; sprite frame pixels
remain screen-aligned billboards, not isometric geometry.

## Reproduce the example and evaluate it

For a quick metadata check on a private local pack, substitute its directory
name below. This inspects the manifest only; `make assets-verify` remains the
integrity check for all page files.

```sh
PACK=local-assets/packs/<input-hash> python3 - <<'PY'
import json
import os
from pathlib import Path

manifest = json.loads((Path(os.environ["PACK"]) / "manifest.json").read_text())
source = "graphics.drs:[32, 112, 108, 115]:3008"
frames = sorted((frame for frame in manifest["frames"]
                 if frame["source"] == source), key=lambda frame: frame["frame"])
assert len(frames) == 50 and [frame["frame"] for frame in frames] == list(range(50))
for frame in frames[::10]:
    print(frame["frame"], frame["page"],
          (frame["width"], frame["height"]),
          (frame["anchor_x"], frame["anchor_y"]))
PY
```

1. Run `make assets-verify` for the local pack. In the viewer, filter `3008`,
   inspect frames 0–9 and the first frame of each subsequent row; compare
   `3004` standing. Check dimensions, anchor, mask coverage, and shadow.
2. Start the game with `AOE_ASSET_PACK=local-assets/packs/<input-hash> make dev`
   and open `/`. Move the rider through all eight facings while panning and
   zooming. Watch that its contact point stays anchored and animation loops
   without disappearing or changing scale unexpectedly.
3. Validate rendered pixels and interaction with `make test-e2e` after client
   or renderer changes. Browser tests cover WebGPU and Canvas 2D startup;
   they do not prove that a private trial pack is visually perfect. Record
   manual real-pack observations separately.

For future units, discover IDs and frame organization in the actual pack,
define an explicit animation/direction table, validate every referenced
frame, then reuse the same placement and atlas-sharing principles. A larger
map or more units should change visible-instance work, not require loading
the whole historical asset catalog into GPU memory.
