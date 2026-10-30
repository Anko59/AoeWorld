# Definitive Edition conversion

Every spawned unit kind, worker role and wildlife pose uses one DAT-driven
importer. The 164 unit poses resolve to 153 graphics and retain 90,102 native
source cells. `make assets-unit-table-de` regenerates their numeric metadata,
while `make assets-import-de` writes the private pixel pack. Semantic bindings
name unit IDs and actions; image sources, drawable deltas and timings come
from the DAT. See [the unit contract](units.md) for lifecycle selection.

## Source resolution and timing

The installed graphics inventory has x1 hero SLDs only. The x2 inventory is
limited to small-tree variants; there is no higher-resolution version of the
selected hero. Consequently x1 is the highest source resolution available for
this conversion. Preserve every native pixel, crop transparent margins, adjust
the hotspot, and never scale down to make the atlas fit. Source images, derived
packs, previews and source digests stay in ignored local storage.

The importer checks the selected graphic records in `empires2_x2_p1.dat`.
The 16 native headings each contain 45 idle, 30 walking, 45 attack, 45 death and
30 decay cells. All source cells are retained, including each SLD's extra final
cell; runtime timelines address the declared direction blocks. Native durations
are 3.5/45, 1.1/30, 1.6/45, 1.85/45 and 1 second respectively. Rounded catalog
milliseconds remain descriptive; playback uses fractional DE durations.

## Bounded residency

Unit cells use on-demand residency rather than startup placement of every
animation. Each independently compressed cell retains colour, owner weight and
shadow planes in bounded, hashed pack chunks. The browser requests current
visible cells first and two phases of lookahead in the current direction.

A tile allocator admits these cells to paired colour/mask pages. Each render
turn protects current visible cells, evicts least recently used unprotected
cells when necessary, and clears every evicted address before reusing space.
Four page pairs add 128 MiB to the base eight layers (128 MiB), for a normal
256 MiB atlas payload independent of the number of imported animations. The
existing absolute renderer cap remains 64 layers / 1 GiB. Driver allocations,
framebuffers and upload scratch are additional allocations, not included in
these texture payload figures.

Encoded cache data plus request reservations are capped at 96 MiB, with at most
four concurrent requests and 64 MiB in flight. A render turn decodes/uploads
at most sixteen cells / 8 MiB, allowing one individually bounded larger cell.
Requests are hashed and bounded before decompression; stale completions belong
to their original asset state. WebGPU and WebGL2 receive subrectangle uploads;
Canvas and context restoration retain the same current physical pixels.

This design bounds the asset working set. Synthetic tests do not establish
finished RTS performance or dedicated hardware qualification.

## Validation and measurements

Synthetic fixtures cover source topology, all native direction blocks, phases
above 31, fractional animation/death timing, atlas growth, aligned masks and
malformed input. `make test-assets-de` also reads the licensed hero's real DAT
and all five SLDs, requiring distinct animation cells in every direction.
`make test-assets-pack AOE_ASSET_PACK=...` proves complete real metadata placement
against the same runtime packer. Real assets never become portable fixtures.

Per-commit measurements, commands and local video paths are recorded in
`reports/assets/de-full.json`. The General conversion's DE overlay is about
131 MiB, compared with the earlier approximately 103 MiB overlay. This does not
include the trial base pack, which is still required by other art categories.

The first same-backend local comparison connected in 23.28 seconds after
conversion versus 60.78 seconds in the retained before build. Owned GPU texture
payload was 340,152,320 versus 138,825,728 bytes (320 versus 128 MiB of atlas,
plus render targets). Median castle-view frame-callback CPU time was 9.8 versus
14.8 ms; this excludes GPU completion. These single shared-machine software
WebGPU runs are observations, not a controlled benchmark. The retained before
binary's exact source revision is unverified; the local report records that limit.

General footage uses ordinary training and game orders. Performance gates use
their existing synthetic workloads and do not qualify dedicated RTS hardware.
## Siege stones

Mangonel stones now use `p_mangonel_x1.sld`, retaining all 30 native rotation
cells in one direction row. Playback uses elapsed authoritative flight time
and the DAT's fractional 1/60-second cell duration. The scene stops drawing a
shot at its authoritative impact tick. Native source resolution is retained;
the installed higher-resolution variants are small trees, not projectiles.

The DAT has two graphics with this debug filename. Select sprite 4500 explicitly
and reject ambiguous identities, changed timing or topology. The importer
requires the source and all 30 cells; the client also rejects partial or extra
cells. Renderer tests draw the final cell from the single row, while scene tests
visit every phase over two loops before checking impact removal.
`make test-assets-de` includes the real stone source alongside hero and house
checks. The current simulation has a Mangonel, with no Onager or Scorpion unit
kind to qualify. Other unit, projectile and trial dependencies remain active work.
