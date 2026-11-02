# Asset import changes

Treat every offset, length, dimension, and command as untrusted. Preserve
frame and mask information; reject unsupported data explicitly. Keep original
assets outside Git and public artifacts. Read [assets](../../docs/assets.md)
and run the changed parser's fixtures with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-assets AOE_NATIVE_TEST_FILTER='test(<parser>::)'` (for example `test(dds::)`). The game data decoder is in `src/dat`.
