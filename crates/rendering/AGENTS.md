# Rendering changes

Prefer the WebGPU backend, with Canvas 2D compatibility for the playable game
when WebGPU initialization fails. Share sprite layout and assets across backends. Preserve
resource accounting and run actual browser rendering tests, not compile-only
checks. Read [rendering ADR](../../docs/adr/0004-rendering.md).
