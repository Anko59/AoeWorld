# Definitive Edition conversion

The General is the first full native animation conversion. Its mounted Frankish
Paladin hero has separate idle/walk/attack/death/decay sources. No trial or Joan
source supplies these poses. The old renderer stopped its pose lookup before
the General and fell back to Scout; all five General tags now resolve directly.

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

Runtime placement uses the logical packing implementation with a 65,536-frame
metadata limit. Two terrain pages precede object pages. The first five colour
pages and their three owner masks keep their base addresses. Additional object
colour/mask pairs occupy 8/9, 10/11 through 62/63. Select the smallest page budget
that packs the current content. Reject content beyond 64 physical layers rather
than allocating without a bound. Alias identical masked/terrain source cells
with ordered-map lookups; shared recoloured cells remain distinct.

Only the required layer count is allocated in the GPU texture array. WebGPU
fetches each source page's three PNG planes once, then composes bounded batches
and blits their rectangles into resident layers. CPU pixel scratch is at most
80 MiB (three input planes and two output planes); one reusable two-plane GPU
texture adds 32 MiB during initialization. Each submission completes before
the next batch, bounding pending driver transfers. The temporary texture is
released when initialization finishes. WebGL2
uploads sequentially, retains its WASM restoration source and restores the same
layer count after context loss. Canvas uses that same physical addressing and
retains its raster source. The base atlas is 128 MiB; the absolute 64-layer
payload bound is 1 GiB, excluding staging, driver overhead and framebuffers.
Owner masks avoid an artwork copy for every player.

This is startup residency for selected content. It does not yet evict offscreen
animations or establish a finished 1,000-player memory/performance result.
The remaining full-unit/building/terrain conversion must measure its working set
and add visibility-driven eviction if it exceeds the bound.

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
The rest of the conversion, siege projectiles and removal of trial dependencies
remain active work.
