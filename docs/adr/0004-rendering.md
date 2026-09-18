# 0004: WebGPU-first browser rendering

Use Rust/WASM with wgpu's WebGPU backend and instanced sprites as the preferred
renderer. The client remains single threaded.

The first playable feature exposed a compatibility gap: successful tests with
forced software WebGPU did not establish that ordinary browsers could start the
game. The game therefore falls back to Canvas 2D when WebGPU initialization
fails, including a missing adapter or a null canvas context. Both backends
consume the same sprite layout, imported atlas, and deterministic simulation.
A failed WebGPU context may bind the original canvas, so replace that element
before installing input handlers when falling back.

Diagnostics continue to require WebGPU. Test their explicit capability error.
Test the game both with WebGPU and without special GPU launch flags, including
missing API, null context, and adapter failures. A compatibility test must
verify rendered pixels and unit movement, not merely an error message.
