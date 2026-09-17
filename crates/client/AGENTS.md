# Browser client changes

Keep WebGPU as the preferred renderer and the client single threaded.
The game must also remain playable through its Canvas 2D compatibility path.
Recover from unsupported capabilities and device failures. Run real browser
tests in addition to WASM compilation; see [testing](../../docs/testing.md).
