# 0004: WebGPU-first browser rendering

Use Rust/WASM scene construction with WebGPU instanced sprites as the preferred
renderer. The client remains single threaded. A thin JavaScript adapter owns only
WebGPU calls; Rust supplies the same readable WGSL, 112-byte packets,
visibility, layer ordering, depth normalization and bounded instance accounting.
The adapter replaces the former wgpu browser wrapper, not the WebGPU tier.

The WebGPU adapter preserves the pinned wgpu 30 browser defaults: no optional
features, the same requested limits, preferred RGBA8/BGRA8 canvas format, opaque
presentation, nearest array sampler, Depth24Plus and LessEqual depth tests.
Its diagnostic atlas is still one 8-square layer; gameplay uses three 2048-square
pages. The 64 MiB instance bound and historical seven-resource metric remain
unchanged. Uploads synchronously consume borrowed WASM bytes without retaining
another CPU packet or restoration atlas. Real browser acquisition succeeds or
reports a lost surface requiring reload; there is no automatic device recovery.
Initialization failure still tries WebGL2 and then Canvas, replacing a bound
canvas before fallback. Pixel fixtures use the very same production pass and
bounded probe copies, not an alternate Rust graphics implementation. Readback
allocations are explicit test resources and are destroyed separately. The entire
adapter JavaScript, including its readback helper, counts in deployed artifact
inventories; WASM size alone cannot establish total transfer savings.

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
Selectors append at byte 96 of the 112-byte ABI (primary, two blends, reserved
zero); storage/attribute capacity remains derived from the unchanged 64 MiB cap.
WebGPU uses D2Array, WebGL2 uses integer attribute 6 and TEXTURE_2D_ARRAY, and
Canvas samples page-major pixels with lazily created legacy page canvases.
Diagnostic WebGPU still uses one 8-square layer (256 bytes), with actual texture
size/layers reported separately from gameplay. GL/Canvas have no GPU counter
observation API; unavailable measurements must not become fabricated counters.
48 MiB is not total memory: source decode scratch, GL restoration ownership,
Canvas copies, transient uploads and presentation buffers remain separately real.
Synthetic page/depth/blend/restoration pixels do not qualify France or hardware.
