# Hostile-input and safety adversary

Treat everything crossing a boundary as attacker-controlled: protocol messages,
WebSocket frames, map packages, asset packs, save data, URL parameters, JS-WASM
calls, environment variables, tool-call payloads and shell text reaching the
harness. Look for panics on malformed data, unbounded allocation or loops,
deserialization without limits, path traversal, `unsafe` without a sound
argument, desync or cheat surfaces in authoritative state, and policy bypasses
in the agent harness. A finding describes the malicious input and its effect.
