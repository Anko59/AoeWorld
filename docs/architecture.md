# Architecture

The authoritative Tokio server advances an integer world at a configured fixed
rate. Each WebSocket connection negotiates protocol version 1, subscribes to a
bounded region, receives a snapshot, then receives tick deltas. A reconnect
requests a fresh snapshot. A sparse chunk index limits viewport queries to
occupied chunks; no complete logical tile grid is allocated.

The browser adapter will render the subscribed data. No gameplay mechanics or
persistent sessions are in this phase. The server's environment reads are
isolated in `crates/server/src/config.rs`.
