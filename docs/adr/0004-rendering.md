# 0004: WebGPU-first browser rendering

Use Rust/WASM with wgpu's WebGPU backend and instanced sprites as the preferred
renderer. The client remains single threaded.

The first playable feature exposed a compatibility gap: successful tests with
forced software WebGPU did not establish that ordinary browsers could start the
game. When WebGPU initialization fails (including a missing API, adapter, or
null context), try WebGL2 instancing before the retained Canvas 2D software
fallback. Full-world CPU rasterization caused measured camera and scout stalls
on the real source map; accelerating the same geometry avoids those stalls
without reducing resolution, terrain detail, or simulation tick precision.
All three tiers consume the same sprite layout, imported atlas, and deterministic
simulation. WebGL2 uses the 112-byte Sprite ABI and equivalent shaders for
per-pixel depth, ordered equal-depth ties, alpha discard/blending, native terrain
UVs, three-material blending, and water tint. Its thin JavaScript adapter owns
only graphics API calls; scene construction and depth normalization stay in Rust.
A failed GPU context may bind the original canvas, so replace that element
before attempting the next tier and before installing input handlers.

WebGL2 requires a 24-bit depth buffer and preserves the existing 64 MiB instance
and 4,194,304-pixel backing limits. Instance uploads borrow a bounded WASM memory
view for a synchronous graphics call; never retain the packet or await while
using it. WebGL copies it into its instance buffer before the borrow ends,
without another retained CPU packet. One additional owned 48 MiB source atlas supports
context restoration without cloning on restore.
Restore shaders, buffers, texture, and atlas on the original canvas, then
invalidate the frame cache so an idle scene also redraws. A lost/zero-size or
skipped GPU frame is not a successful presentation: do not advance picking or
render-cache positions until pixels have actually been submitted.

Diagnostics continue to require WebGPU. Test their explicit capability error.
Test the game both with WebGPU and without special GPU launch flags, including
missing API, null context, and adapter failures. A compatibility test must
verify rendered pixels and unit movement, not merely an error message.
Explicit Canvas 2D tests deny both GPU APIs; hiding WebGPU alone now exercises
WebGL2, not software compatibility. Mandatory WebGL2 tests render crossing
surfaces, transparent occluders, depth ties, native material blending/tints,
mirrored units, and real movement, plus bound-context initialization failure and
context loss/restoration. Synthetic pixel tests do not establish real-source
frame cadence or dedicated-hardware performance; measure those separately.

## Immediate natural-surface appearance corrections

Scene material 4 is explicitly procedural neutral rock, using native dirt detail
with desaturated bounded luminance; imported paving is not natural rock art.
Non-water cliff tops and edge faces use this appearance at every mesh LOD.
Materials 7–10 are respectively restrained procedural snow, interim cool ice,
wet mud, and subdued shallow/shore water. Snow uses dirt detail and is not ice.
These appearances share the same fragment transform in WebGPU, WebGL2, and
Canvas; source recipes, heights, picking, and simulation remain unchanged.
Missing forest art uses dirt uniformly; present forest variants never alternate
with dirt on an 8×8 grid. No atlas allocation or source-art editing is involved.

Procedural boundaries temporarily disable three-material splatting because the
existing packet carries one transform per face, not one per material. Native
materials still splat normally elsewhere. Explicit catalog topology reaches
GameArt; frame count alone never establishes a periodic sheet.
Forest accent selection is intermediate: removing parity does not establish
seamless full-sheet suitability. Stage 2 needs full-sheet review or accents with
edge masking over a coherent base. All procedural ramp appearances preserve
0.92 face shading through reserved tint codes 21–26 (base codes 5–10 plus 16).
Cliff tops retain 0.78 shading with code 12, distinct from skirt code 7's 0.72;
flat rock remains code 5. Unknown code 11 retains its original passthrough.
The CPU and both GPU shaders combine shading before their single output rounding;
alpha, atlas allocation, and the Sprite ABI remain unchanged. Coarse cliff
geometry still shares averaged heights and omits skirts; this stage changes its
appearance, not its geometry or dedicated-source qualification.

## Bounded multi-atlas addressing

Gameplay owns exactly three 2048-square RGBA pages (48 MiB base pixels): terrain
0/1, objects/units/shadows 2. Source page numbers are never runtime page numbers.
Required overflow fails; never discard visible trees or regroup painter order.
Page and UV travel together through shared terrain samples and sprite frames.
Selectors append at byte 96 of the 112-byte ABI (primary, two blends, appearance
word); storage/attribute capacity remains derived from the unchanged 64 MiB cap.
Sprites without landscape appearance keep a zero fourth word.
WebGPU uses D2Array, WebGL2 uses integer attribute 6 and TEXTURE_2D_ARRAY, and
Canvas samples page-major pixels with lazily created per-page canvases.
Diagnostic WebGPU still uses one 8-square layer (256 bytes), with actual texture
size/layers reported separately from gameplay. GL/Canvas have no GPU counter
observation API; unavailable measurements must not become fabricated counters.
48 MiB is not total memory: source decode scratch, GL restoration ownership,
Canvas copies, transient uploads and presentation buffers remain separately real.
Synthetic page/depth/blend/restoration pixels do not qualify France or hardware.

## Landscape appearance

Map chunks always carry canopy/floor strength, palette, exposure and height band.
Scene terrain without that metadata (synthetic, non-map scenes) is None and uses
the base material kernel. Displayed triangles retain a packed
u32 sidecar (private field, not a public DTO change), with zero for None and a
presence bit for Some zero-strength metadata. Canonical nearest world-cell-centre
ownership with tile-key tie breaks
makes coarse appearance independent of source order and fine camera visibility.
Geometry and canonical appearance share one input traversal and the same bounded
CellSample candidate storage/comparator. Appearance ownership is selected before
fine visibility rejection; geometry still applies its existing visibility test.
No resident-world appearance map is cloned and no geometry/picking budget changes.

Vegetative terrain packets use pages.w: presence bit 0, palette bits 1–3, floor
strength bits 4–13, canopy bits 14–23, exposure bits 24–25, height band bits 26–28.
Bit 29 marks interpolated floor endpoints; bits 30–31 remain zero. Some zero
strength still has presence set. Protected procedural
rock/snow/ice/mud/water, sand and skirts retain their existing kernels and zero
packet word; their raw scene/triangle metadata is not discarded. Exposure and
height are carried semantic evidence, not species/art selection permissions.

New vegetative faces blend grass (dry grass in dry/savanna regions) or
authoritative dirt with native 15011 forest-soil leaf litter when its reviewed
optional frames are present. The earlier coherent-dirt substitution was rejected
in live visual feedback: dirt is not forest soil. The ten coordinate-stable native
frames restore the intended material, not seamless periodic-sheet qualification.
Absent optional forest frames fall back to a dirt bed; metadata-free material 6
keeps its base selection. Native forest soil is available
even if the dirt group is absent. Identical primary/bed addresses still use one
sampler rather than three duplicate reads; their appearance transform remains
active without an interpolated blend flag. Canonical displayed integer vertices gather a rounded mean of
up to four incident landscape vegetative cells, excluding missing, metadata-free, cliff and water
support; no support falls back to zero. Duplicate cell records cannot reweight the
mean, and conflicting duplicates conservatively choose the lower floor. Shared
vertices agree across input order and triangulation; later loaded source support
can refine an incomplete halo. Only floor strength is continuous here: palette,
canopy and authored accent texture seams are not solved by this field.

The triangle endpoint sidecar stores three quantized u8 values and costs 4 bytes
per face (98,304 nominal field bytes at 24,576 faces). On the pinned WASM target,
the enclosing triangle remains 240 bytes because alignment absorbs the reduction
from the former 8-byte sidecar; this is not retained triangle-vector byte savings.
Four u16 support slots with an explicit missing sentinel cost 8 bytes per unique
displayed vertex, at most 73,728 vertices/589,824 live payload bytes; vector capacity
growth and the world-key index are additional real CPU storage. Each shared mean
is computed and quantized once per canonical vertex. Canvas normalizes endpoints
once per surface rather than quantizing them in every fragment.
No resident-world terrain map is cloned. The 112-byte Sprite ABI is unchanged.
For bit 29 packets, reserved surface depths.w carries three rounded u8 endpoints
as an exact 24-bit numeric f32; depth normalization explicitly preserves that word.
GPU vertex stages and Canvas interpolate the same quantized endpoint strengths.
Weights are (1-floor, floor, 0), not base-material barycentric weights. Synthetic
or manual packets without bit 29 use the face-constant floor/1000 fallback. The primary
and both secondary page addresses still travel together without extra geometry,
atlas allocations or draw-order regrouping. Restrained palette channel
multipliers and canopy shading (at most 12%) follow face lighting, with shared
byte rounding in Canvas, WGSL and GLSL. This does not qualify forest accent sheets
as seamless full-sheet art; that review limitation above remains in force.

Eligible landscape vegetative terrain alone uses a bounded minification kernel when
source footprint exceeds 1.25 texels per backing pixel. Four symmetric pixel-quadrant
RGB samples stay clamped to the same atlas rectangle's texel centers; their RGB
is alpha-weighted and byte-rounded before tint/appearance. The original nearest
center alpha stays exact, preserving coverage and depth; zero summed sample alpha
or nonfinite rect/UV/derivatives falls back to that center. GPU derivatives are
evaluated before branches/discards, and three additional flat rectangle varyings
travel within the existing interstage limits. No Sprite112 or atlas allocation changes.
Canvas computes gradients and offsets per surface, not per fragment: at most two
64-byte offset payloads, plus Option tags/padding and 32-byte gradient storage;
compiler stack/register layout and call arguments are additional real storage.

A filtered single layer costs up to 5 atlas reads (center plus four quadrants),
landscape blends up to 10; their zero third weight avoids a third sampler. Metadata-free,
manual zero-word, protected faces and every sprite/shadow/selection retain
nearest sampling (one read, or three for base-material blends). New
cross-backend pixels pin contrast attenuation, nearest/magnified controls,
transparent/partial-alpha behavior, rectangle/page isolation and unchanged depth
bits/picking. This is not a full mip pyramid, severe-zoom antialiasing guarantee,
frame-cadence or dedicated-hardware qualification.

## World-anchored terrain texels

A world tile shows the same native frame and texels at every zoom and mesh LOD.
A face covering one tile keeps its CPU-selected rectangle. A coarse face covering
several tiles (texture mode 6) must not stretch its origin tile's frame over all
of them, which changed pattern scale with zoom and popped at LOD switches (#213).
Its packet carries the three corners in tile units (uv, terrain_blend[0].xy), the
origin tile (pages.x/y as signed bits) and its three layer materials (pages.z
bytes). Each fragment floors its interpolated position to a world tile, selects
that tile's frame by the authored sheet phase or the coordinate hash, and samples
the native diamond at the tile-local position. Blend weights, floor endpoints,
tints, the appearance word and the bounded minification kernel are unchanged;
the kernel's footprint comes from the continuous tile-space derivative, and its
taps stay clamped to the selected frame.

Frame rectangles come from a terrain directory inside the atlas itself. The
packer never allocates rows 0–1 of a page (placements start at texel 2), so
`write_terrain_directory` stores, in those rows of page 0, two RGBA8 header
texels per scene material (first entry, frame count, sheet columns and rows)
and two texels per frame (origin; extent and page). At most 4,095 entries
(16,380 bytes) of existing atlas storage are used: no texture, binding, uniform,
upload, Sprite112 field or interstage varying is added, and WebGL2 restoration
re-uploads the same bytes. `GameRenderer::upload_game_atlas` writes it from the
art; occupied reserved rows or unaligned terrain rectangles fail the upload, and
art without terrain frames writes nothing. WGSL, GLSL and Canvas decode the same
bytes; the vertex stage reads the three headers once per face, the fragment two
directory texels per layer. Frame selection uses unsigned arithmetic so signed
tile coordinates wrap identically on every backend. A tiled face whose atlas has
no directory draws nothing instead of a wrong frame.

Cross-backend pixels pin a periodic sheet and coordinate-stable accents at a
near and a far zoom, one tile per face against 2×2-tile patches under one camera,
and a filtered forest-floor bed layer. Coarse heights, per-cell appearance and
per-vertex material ownership still follow the mesh LOD; only texel anchoring is
fixed. Without a mip pyramid, far-zoom texels remain point/quadrant sampled.
The map-less diagnostic background still scales its sprites by aggregation cell.

Resource family zero (`Generic`, non-tree resources) keeps the base variant
selection. Wood family 2 uses
reviewed conifer 4654 healthy indices [1,2,3,4,7,8]; family 4 uses reviewed palm 4653
single indices [0,1,2,3,5,6,8,10,11,12]. Full raw prefixes 9/13 live in optional
GameArt tree_families, separate from the four economic resource kinds. Only an
empty species group falls back to healthy broadleaf 4652 with matching 2296; a
nonempty malformed prefix is unavailable, not absence. Other families retain
healthy broadleaf fallback and nonwood ignores family metadata. Central
scene_resource_presentation chooses body and shadow together; culling's body
wrapper and drawing share that selection. Native species use the existing selected
body alpha silhouette, never 2296 or unqualified 2304/2300. Signed/outside anchors
are preserved. Culling remains body-only: shadow-only edge envelopes are not fixed.
No IDs, kinds, amounts, passability, generator bytes or source identities change.
Present species may move atlas UVs through repacking; broadleaf atlas addresses
stay fixed only when both new sources are absent. No dry-scrub or cliff art is
approved by its semantic name. Decorations
retain a distinct scene DTO and visible client collection, but are deliberately
omitted from drawing while reviewed decoration mappings remain empty. They are
never proxied into resources, gatherables, blockers or economy objects.

## Ground mixing between every material pair

Only grass against forest soil used to blend: map tiles always carry appearance
metadata, and that branch of `apply_terrain_textures` chose grass plus the
forest bed and returned before the per-vertex material splat; rock, snow, ice
and mud faces dropped their vertex materials because their procedural look was
one tint per face. `surface_mesh/mix.rs` replaces both gaps for map scenes.

Each displayed vertex takes the share of displayed ground and shallow-water
sheets among its four incident source cells. Metadata-free samples are not
observations. A quad whose four corner supports see one sheet keeps its
existing path. A face in a quad that sees several becomes a world-tiled face (mode 6) with
layers ordered by `mix::rank`, low to high: ice, rock, gravel, mud, sand, dirt, grass and dirt mix, dry
grass, worn grass, grass, lush grass, snow. The lowest is the base and the highest the overlay, so both
sides of a border draw the same pair. The representative palette comes from all
four quad corners in that fixed priority order, so its two triangles agree. Ordinary borders sample the base,
a middle material or forest-floor bed, and the overlay. A quad with more than
three ground sheets, or three sheets plus a bed, retains every vertex observation
and evaluates every nonzero ground layer before applying the bed.

Ordinary corner shares are u8 (k/4 of the incident tiles) and travel in the tiled
packet's free words: `terrain_blend[0].z` is the mode (1 bed, 2 middle
material) and `terrain_blend[1].xyz` one exact integer per corner, overlay
share plus 256 × middle share. Bed strengths reuse the interpolated floor word.
The fragment thresholds the interpolated share `s` with world-anchored noise
`n`: `clamp((s - 0.5) * 4 + 0.5 + (n - 0.5) * 2.6, 0, 1)`. A share of zero or
one stays exact for any noise, so pure ground is untouched and the band is
about half a tile soft, meandering up to 0.6 tile around the tile edge. `n` is
two octaves of value noise on integer lattices (one and four cells per tile)
hashed from the fragment's world tile, identical in WGSL, GLSL and Canvas and
at every zoom and mesh LOD. Weights are cumulative thresholds (`1 - m`,
`m - o`, `o`) for three materials and `(1-b)(1-o)`, `b`, `(1-b)o` with a bed,
so a face with two materials meets a face with three without a seam.

Junction mode 3 stores four material nibbles per vertex (15 means absent) in
`terrain_blend[1].xyz`. Two spare words, `terrain_blend[0].w` and
`terrain_blend[1].w`, carry up to twelve ordered material nibbles as exact
24-bit integers. Mode 3 reuses the unused overlay-material byte for the palette
count; the fragment loop uses that varying bound to avoid twelve-way filtered
sample unrolling in drivers. The vertex shader passes the observations and order flat in
the existing rectangle varyings, and barycentric coordinates plus bed strength
in the existing weights varying. Each fragment reconstructs the same rounded
corner shares and cumulative thresholds as ordinary borders. Zero-share layers
have zero weight, so differing off-edge materials cannot change shared-edge
colours. Canvas uses the same observations and order. The triangle reuses its
unused third rectangle for these observations and remains 240 bytes.

Procedural rock, snow, ice and mud are evaluated per layer from the layer's
scene material (the packet's material bytes) before blending; face tints 5–10
remain only for faces that are entirely one procedural material, and the two
agree where they meet. Mixed faces keep face shading (tint 0–3), the minification
kernel on every sampled layer, and the ecological palette only when the face's own
tile is vegetative. Ordinary mixed faces pay two directory texels and up to
five taps per contributing layer plus eight lattice hashes when a share is
partial. Pure shares and a complete bed need no lattice evaluation. Junctions decode
three four-nibble observations per layer and resolve each nonzero sheet.
No Sprite112 field, texture,
binding or varying is added, and the triangle stays 240 bytes (six spare bytes
hold the shares). Shallow water and ford shallows also contribute shore observations;
loaded medium/deep water completes their halo without blending a direct
land/deep-water edge. Borders are still one mesh face wide on coarse
LODs, and long straight biome edges are a map-generation property.

The shared shader compiles into ordinary and ground-mixing programs. SwiftShader
showed substantial ordinary-frame overhead from the larger fragment program
even when its mixing branch was inactive. Rust build-time source assembly removes
mixing-only WGSL sections before module compilation; the ordinary module has no
mixing functions or junction branches. GLSL specializes with a compile-time
constant. Contiguous instance runs
choose their program without regrouping or changing painter order, depth ties,
or alpha coverage. Both programs use the same buffers, atlas, bindings and
Sprite112 layout. WebGPU creates the mixed pipeline only when needed and counts
that additional persistent resource; draw-call counters count the actual runs.
WebGL restores both programs and the retained atlas on context restoration.

Mixed layers share one filtered sampling body and skip zero-weight layers before
frame resolution and texture reads. Tap offsets come from loop bits and
procedural kinds from a switch, avoiding indexed fragment temporary arrays.
Canvas makes the same ordinary-mix weight decisions before sampling. The exact
two-octave field remains per fragment to retain the existing noisy-border pixels.
The earlier dense-sprite timing regression measured 526.5 ms for production
against 142.8 ms for a one-read oracle locally. Shared-runner scheduling makes
that ratio unsuitable as a test verdict. Its replacement inspects the exact
WGSL passed to WebGPU, excluding mixing helpers/branches from the ordinary
module and requiring one sample with no directory reads in the sprite arm.
Actual renderer runs and equal-depth alpha readbacks pin program selection and
painter order. These deterministic contracts do not qualify frame cadence or
dedicated-hardware performance.

Scene materials 13–16 support ground variety (grass and dirt mix, worn
grass, lush grass, gravel), drawn from optional art groups 7–10; a pack without
a sheet draws dry grass, grass, grass or sand. The three grassy ones carry the
ecological palette like grass; none is procedural. The directory holds seventeen
material headers. Junction nibbles 5 and 6 encode scene materials 15 and 16;
medium/deep classes are halo markers rather than junction layers, the forest
bed never supplies an observation, and 15 stays empty.

## Direct scene emission

The production scene builder now appends the shared sprite emitter directly
after surfaces and selection rings. It no longer materializes an intermediate
object vector and then copies it into the WorldLayer vector. Resource-before-unit
submission and the existing stable-index depth/type/id sort remain unchanged.
The diagnostic terrain-frame helper, selection temporaries, integer sort sidecar and GPU
instance vector still allocate; this is not whole-renderer allocation reuse.
Bounded scene tests pin the final layer content over 72 cases (missing art,
source ties, mirrors, boundary cameras, NaN depths) by digest and assert the
stable depth/type/id order. An absolute allocation budget for a dense
8,158-sprite scene (no growth reallocations, at most 102 allocations) guards the
direct path. Performance/size claims require measured unchanged gates.


## Soft shallow shorelines

Shallow water and ford coverage overlays the ground shares with a linear,
half-tile transition. A shore quad needs complete source support; missing cells
never count as dry land. Medium/deep source cells complete the shallow halo,
while their own displayed faces keep the depth-sheet path. Vertices touched by
a blended shore quad pin adjoining native water faces to the same shallow sheet;
the existing depth interpolation resumes farther into the water. A temporary
byte per requested vertex records these pins only when shallow water is visible.
Source water flags,
passability and picking ownership remain unchanged.

Ordinary two-sheet shores reuse the interpolated floor endpoint word with
neutral palette six, keeping the ordinary shader and instance run. Ground
junctions and procedural shores use the existing mixing program. No atlas,
Sprite112 field or inland geometry is added. A dry triangle whose opposite quad
corner alone sees an overlay retains only the base sheet: zero coverage must
never fall back to a three-vertex splat and draw a hard triangular patch.

Shore skirts are omitted only when shared-edge heights are equal. Unequal tops
keep the closing strip and the ground bank's material. One-level beach strips
use neutral shading and the tops' shared shoreline coverage with complete source
support, avoiding a dark wall or a background hole without changing source heights.
Larger drops retain their bank shading.
A generic grass placeholder on a bank falls back to neutral rock detail.
Synthetic mesh/pixel tests cover these contracts across Canvas, WebGL2 and
WebGPU; prepared-map screenshots and frame samples are separate qualification,
not dedicated hardware or finished RTS performance evidence.

## Village work and desktop terrain fidelity

Villager scene poses follow authoritative task/resource state: partial loads
keep their gathering animation; only Returning selects food, wood or shared
ore carry art. Nine optional native groups preserve five 15-frame rows, signed
hotspots, eight-direction mirroring and stationary-carry behavior.
Town-center wings draw behind the core; reviewed roof/post attachments join the
native parts without changing their shared ground origin. Footprint wear shades
existing face colors, retaining geometry, material selection and picking.

Slope shading makes terrain relief read (#428). Each ground vertex brightens or
darkens by the slope of the displayed heights around it toward a north-west
light (the top of the isometric screen, the minimap's convention): 4 % per step,
-8 to 7 steps, zero on flat ground, water, cliffs and native cliff art. A cell's
shade is a local plane fit (5 tiles) minus 80 % of a broad one (20 tiles), so a
whole hillside keeps its average brightness while hills and valleys keep their
contrast on maps whose slopes are tiny at their scale. A lattice corner takes
the mean of the cells around it, so faces sharing it agree, and the shaders and
the Canvas raster interpolate the three vertex codes across each face (no
facets). The codes live in the low mantissa bits of the face's colour channels,
flagged in its draw order, and travel above the footprint wear in the packet, so
the packed triangle stays 240 bytes; the three renderers share one kernel
(`surface_mesh::relief`). The gradient is in height levels per tile, so averaged
far-zoom cells and near-zoom ramps agree.

Minimum zoom first budgets the actual visible source cells and their closing
edges rather than treating every cell of their rectangular index as drawable.
When they fit the unchanged 24,576-triangle cap, source shores, materials and
heights agree with the closer level. The rectangular index is bounded to at
most 24,576 slots; at most two top and four closing-edge triangles are counted
per visible cell. Larger views retain the existing coarse fallback. This does
not qualify dedicated hardware or finished RTS performance.

Fog object lookup shares the existing integer-pair index, splitting each full
64-bit identity into two words. Parallel temporary values replace two additional
BTreeMap specializations in the WASM bundle. Resource body/shadow keys retain
bit-exact atlas coordinates and newest matching updates take priority over unit
IDs. Storage covers displayed resources and units only: at most one index/value
per unique ID and one paired-frame record per resource, with geometrically
growing vector capacity and index slots as additional real memory. This is a
code-size change, not a blanket RAM reduction. The browser regression measures
8,192 paired resources plus 1,024 units and checks all 17,408 shaded layers;
synthetic timings do not qualify source-map cadence or dedicated hardware.
