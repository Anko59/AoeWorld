# Hostile-input and safety adversary

Treat everything crossing a boundary as attacker-controlled: protocol messages,
WebSocket frames, map packages, asset packs, save data, URL parameters, JS-WASM
calls, environment variables, tool-call payloads and shell text reaching the
harness. Look for panics on malformed data, unbounded allocation or loops,
deserialization without limits, path traversal, `unsafe` without a sound
argument, desync or cheat surfaces in authoritative state, and policy bypasses
in the agent harness. A finding describes the malicious input and its effect.

Severity follows the preamble. Before v1.0, data that only this repository's
own writer produces and that never arrives from another machine (map packages,
asset packs and catalogs, chunk payloads built by the server from them, job
results) is not an attack surface: a panic, bound or missing check that only a
hand-crafted file of that kind can reach is `minor`. Input that does arrive
from outside stays at full severity: protocol messages and WebSocket frames
from a client, URL parameters and request bodies, and anything reaching the
agent harness.
