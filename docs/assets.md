# Assets

Trial assets and derived packs must remain in ignored `local-assets/`, outside
Docker build contexts and public artifacts. Synthetic fixtures are the default.
`make assets-inspect`, `make assets-import`, and `make assets-verify` operate on
ignored `local-assets/trial` and `local-assets/packs`. The Rust importer bounds
DRS, palette, and SLP decoding, creates deterministic padded PNG atlas pages
with separate player-color masks, and writes a versioned manifest. Synthetic
golden fixtures, malformed-input tests, deterministic import, and pack
verification pass in the normal gates. No extracted trial directory is present
here, so the importer has not been validated against actual trial data.
The original game data license is separate from AoeWorld's MIT code license.
