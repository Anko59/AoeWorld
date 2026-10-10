# Bounded multi-atlas implementation notes

Contract and implementation guidance for the bounded three-page atlas. This is
not a qualification report.

## Fixed contract

- Page side 2,048; RGBA8, nearest filtering, no mip maps.
- Exactly three runtime pages: terrain 0–1; units, resources and shadows 2.
- Base pixel payload 50,331,648 bytes (48 MiB); at most 2,048 selected frames.
- Keep the infrastructure ceiling separate from the current reviewed 656-frame
  catalog. More capacity never approves previously unreviewed candidate art.
- Domain overflow fails even if another domain has space. No dropped visible
  trees, omitted terrain variants or painter reordering to improve packing.

## Addressing and ABI

Carry page plus UV together through GameFrame, projected terrain's primary and
both blend addresses, every sprite, mirrored frame, shadow and synthetic fixture.
Reserved white texels for solid/grid/ring sprites must have an explicit page.
Translate catalog topology into typed GameArt metadata; remove the count-based
100-frame inference and preserve authored sheet phase with signed coordinates.

Prefer appending `[u32; 4]` page selectors at byte 96 of the current Sprite:
primary, blend one, blend two, reserved. The new stride is 112 bytes, with every
old offset retained. Update Rust/WGSL/GLSL together and pin offsets/size. WebGL2
uses a seventh integer instanced attribute through `vertexAttribIPointer`.
Capacity is derived from the new stride while retaining the 64 MiB instance cap.
This is an internal renderer ABI, not a map/protocol/simulation change.

## Loader

Refactor bounded selection, placement and copying into helpers in the existing
client asset subdirectory. Validate selected counts before sorting/allocation.
Maintain semantic frame order independently of the deterministic packing order
(height, width, semantic index). Terrain uses first-fit pages 0 then 1; objects
use page 2 only. Preserve gutters, signed anchors, source masks, partial alpha,
and present-corrupt optional-source rejection. Source page IDs and runtime page
IDs are different namespaces.

Allocate a single flat page-major 48 MiB destination. Decode source pages
sequentially; color/player/shadow scratch coexists, but never cache all decoded
source pages or clone destination pages. Typed topology is independent of page.

## Backends

**WebGPU:** one three-layer RGBA8Unorm texture, explicit D2Array view/binding and
`texture_2d_array` sampling. Upload page-major bytes with `rows_per_image = 2048`.
Synthetic diagnostics retain one small layer, using the same array-compatible
layout rather than allocating 48 MiB. Preserve existing WebGPU loss/error behavior;
do not claim new automatic recovery. Rebinding on instance capacity growth must
retain the array view.

**WebGL2:** TEXTURE_2D_ARRAY, immutable storage and sampler2DArray with flat integer
selectors. Check maximum array layers, texture size and seven vertex attributes.
Retain one owned 48 MiB restoration array; never retain a borrowed WASM view across
memory growth or await. Restoration uploads all layers from the existing owned
array, without cloning it again, then invalidates the idle frame cache. Lost,
zero-size and skipped frames remain unsuccessful presentations.

**Canvas:** one page-major CPU source; checked `page * PAGE_BYTES + local_offset`
for sprites and all three terrain samples. Canvas drawImage needs page-specific
canvases, preferably lazy rather than eagerly retaining three additional copies.
Never make one oversized 4096/6144-wide or tall atlas. The depth raster, picking,
geometry and current procedural-boundary blend policy remain unchanged.

All paths preserve the stable depth/kind/id ordering, shadow/body ties, alpha
handling and equal-depth overwrite. Texture arrays permit the existing single
ordered instanced draw. Never sort or batch by texture page.

## Accounting

Replace hardcoded page/byte/upload counters with actual resource metadata.
Diagnostics remain one page / 256 bytes; gameplay is three pages / 48 MiB. A
texture array is one GPU texture object but three atlas pages.

Base payload is not total memory:

- WebGPU atlas: 48 MiB GPU, plus upload/source decoding transient allocations.
- WebGL2: 48 MiB GPU plus one 48 MiB owned CPU restoration copy.
- Canvas: 48 MiB CPU source, plus up to 48 MiB for per-page drawImage canvases if used.
- Source PNG color/player/shadow decoding: 48 MiB scratch, plus compressed bytes,
  inflater/IDAT/JS buffers; upload may temporarily coexist with CPU and GPU copies.
- Existing backing cap is 4,194,304 pixels / 4,096 per axis; color plus f64 depth
  is 48 MiB and ImageData may add 16 MiB. Terrain cache remains 128 MiB / 512 chunks.

Measure startup, retained and transient separately. Do not relax source memory
RSS, retained-memory variation, WASM growth or performance baselines to absorb
this change. Visible sprite retention is not a memory-saving escape hatch.

## Tests

- Selection 2,048 accepted / 2,049 rejected before allocation; exact reviewed
  group completeness, corrupt optional groups, deterministic ties and packing.
- Terrain spills only into page 1; objects stay on page 2; oversized frame,
  gutters and domain overflow fail explicitly. Preserve mask and anchor bytes.
- Equal UV rectangle with distinct sentinel colors on pages 0/1/2, terrain blends
  across pages, mirrored units and shadow anchors on all three backends.
- Alternating-page transparent occluders, crossing depth and equal-depth ties
  preserve painter order. Restoration tests cover terrain and objects, idle and
  interactive scenes, without duplicate restoration copies.
- Real browser startup, movement, resize, fallback, invalid capability limits,
  diagnostic/game counters and picking/contact parity.
- Browser-only tests use wasm_bindgen_test. The current harness contract catalog
  supports native cases only; do not weaken its schema to insert WASM cases.

## Private source review

Existing Rust harness inspect/import/verify commands and the asset viewer supply
bounded decoding and frame/mask/hotspot inspection. Add full-sheet seamless/wrap
inspection and paired-shadow anchor overlays before promotion. A 64-frame ice
source does not imply an 8×8 grid; equal conifer/shadow counts do not prove pairing.
Cliff placeholders and unusual anchors need explicit usable subsets/orientations.
Original images, contact sheets and screenshots stay private/ignored.

## Validation

Follow current engineering and scoped instructions, exact intended index scope,
Dockerized focused checks, hooks and `make preflight` before commits. Stage-1 map
scope already selects broader native/performance/fuzz gates. Local evidence is
non-authoritative and does not authenticate independent QA or served binaries.
Actual activated source URL/hash and reference packages remain required for
France memory/movement qualification; hardware and CI are separate evidence.
