# Rendering changes

Prefer WebGPU; try WebGL2 acceleration when WebGPU initialization fails,
retaining Canvas 2D compatibility when both GPU tiers fail. Share sprite layout
and assets across backends. Preserve
resource accounting and run actual browser rendering tests, not compile-only
checks. Read [rendering ADR](../../docs/adr/0004-rendering.md).
