# 0002: Versioned Postcard WebSocket

Use a shared Serde/Postcard binary contract over WebSocket, with explicit
handshake, subscription, snapshot, delta, resync, and error messages. Validate
message and region bounds, and disconnect slow consumers.
