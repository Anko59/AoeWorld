# Architecture

The authoritative Tokio server advances an integer world at a configured fixed
rate. Each WebSocket connection negotiates protocol version 1, subscribes to a
bounded region, receives a snapshot, then receives tick deltas. A reconnect
requests a fresh snapshot. A sparse chunk index limits viewport queries to
occupied chunks; no complete logical tile grid is allocated.

The Rust/WASM browser adapter renders subscribed entities through wgpu's
WebGPU backend, using instanced sprites, viewport culling, and a persistent
synthetic mask atlas. Camera movement and zoom change the subscription; two
browser sessions receive independent snapshots and can reconnect. The client
records bounded frame, decode/update, and CPU submission timing samples for
informational hosted reports. No gameplay mechanics or persistent sessions are
in this phase. The server's environment reads are isolated in
`crates/server/src/config.rs`.
