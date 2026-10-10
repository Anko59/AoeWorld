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
coarse-LOD world-frequency fix, frame-cadence or dedicated-hardware qualification.

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
