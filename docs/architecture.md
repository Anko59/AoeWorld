# Architecture

The authoritative Tokio server advances an integer world at a configured fixed
rate. Each WebSocket connection negotiates protocol version 1, subscribes to a
bounded region, receives a snapshot, then receives tick deltas. A reconnect
requests a fresh snapshot. A sparse chunk index limits viewport queries to
occupied chunks; no complete logical tile grid is allocated.

The `/diagnostics.html` Rust/WASM browser adapter renders subscribed entities through wgpu's
WebGPU backend, using instanced sprites, viewport culling, and a persistent
synthetic mask atlas. Camera movement and zoom change the subscription; two
browser sessions receive independent snapshots and can reconnect. The client
records bounded frame, decode/update, and CPU submission timing samples for
informational hosted reports. The main `/` game runs a local single-unit integer
simulation at 50 Hz with click-to-move commands and the shared WebGPU renderer. The client loads the selected local pack,
combines terrain, cavalry, player-color and shadow pixels into one atlas, and
animates five stored directions with mirrored east-facing sprites.
It has no networked gameplay, collision obstacles, or persistence. The server's environment reads are isolated in
`crates/server/src/config.rs`.
