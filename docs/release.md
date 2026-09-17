# Local release rehearsal

Builds start from a clean `dev` commit. Run `make test-e2e` and `make perf-ci`
against that commit, then `make release-build`. The Rust harness rejects stale,
dirty, or failing reports. It compiles the server and WASM client once in a
single artifact image, then creates the server and browser runtime images from
that build. The runtime images run as non-root users with read-only filesystems
in rehearsal.

`reports/release/<commit>/manifest.json` binds the source commit and tree,
local OCI image IDs, exported static bundle hash, protocol and asset-pack
versions, toolchain, and copies of the functional and performance reports.
These reports and the bundle remain ignored local outputs. `AOE_RELEASE_MANIFEST`
selects a manifest for `make release-verify`. Verification rejects changes to
the bundle, reports, image tags, source tree, or format versions.

Set `AOE_RELEASE_CANDIDATE` and `AOE_RELEASE_PREVIOUS` to two different local
manifest paths, then run `make release-rehearse`. The harness starts the previous
images, verifies health and build identity, promotes the candidate, and starts
the previous exact image IDs again as rollback. It creates and removes only its
own Docker network and containers. No live service is changed.

Local image IDs are content hashes on this Docker host; they are not published
registry digests. Registry publication, SBOMs, GitHub OIDC signing, provenance,
release branches, and promotion to `main` still require implementation. The
initial bootstrap commit on `main` was the one-time exception to the intended
PR flow. Protected `dev` and `main` settings will be enabled after branch
policy and required checks are verified.
