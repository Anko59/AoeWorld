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
registry digests. On a verified `dev` push, the `dev-artifacts` job performs
complete functional and target-scale verification, builds the local release
once, generates SPDX SBOMs for both runtime images and the static bundle,
and runs `make release-publish`. The Rust publisher checks the local manifest,
logs into GHCR with the job token, pushes the exact local images, captures
registry digests, and writes `published.json`, `bundle.tar`, and checksums.
Pinned GitHub Actions use OIDC to attest the image digests, SBOMs, bundle,
and published manifest. The job attaches the manifest, evidence, bundle, SBOMs,
and checksums to a `build-<dev commit>` prerelease. It has write permissions
only on the trusted `dev` push, after the aggregate required check passes.

Published digests, signatures, and SBOMs must be verified from the exact
`dev-artifacts` run before they are reported as evidence. A release
branch starts from `main`, is named `release/<verified dev SHA>`, and carries
the exact tree of that dev commit. A PR to `main` runs the read-only release
candidate job: it rejects a different tree or dev ancestry, downloads the
matching `build-<SHA>` assets, checks the manifest, checksums, evidence, SBOMs,
and registry digests, and verifies the OIDC attestations. It starts candidate
images by digest in a disposable stack. When a previous promoted release
exists, it also starts that release and rolls back to its exact digests. The
first release can only test candidate health; its report records
`rollback_rehearsed: false`. Once `main` moves beyond the initial bootstrap
commit, failure to obtain a previous release blocks the rollback gate.

After merging the release PR, the `main-promotion` job finds the unique `dev`
commit with the same source tree, downloads and verifies that existing build,
checks its attestations again, and marks its `build-<SHA>` prerelease as the
latest promoted release. It does not build or retag the images. Check the job
on the exact `main` revision before claiming promotion. The initial bootstrap
commit on `main` was the one-time
exception to the intended PR flow. `make repo-policy-check`
audits the GitHub default branch, squash-only
merges, strict `required` check, pull-request requirement, signed commits, and
force-push/deletion restrictions on `dev` and `main`. Pass `GITHUB_TOKEN` for
authenticated API access when needed.
