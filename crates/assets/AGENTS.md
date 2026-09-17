# Asset import changes

Treat every offset, length, dimension, and command as untrusted. Preserve
frame and mask information; reject unsupported data explicitly. Keep original
assets outside Git and public artifacts. Read [assets](../../docs/assets.md)
and run parser fixtures plus `make preflight`.
