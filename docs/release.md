# Local release rehearsal

Builds start from a clean `dev` commit. Run `make test-e2e` and `make perf-ci`
against that commit, then `make release-build`. The Rust harness rejects stale,
dirty, or failing reports. It compiles the server and WASM client once in a
single artifact image, then creates the server and browser runtime images from
that build. The artifact build also emits the build-assembled WGSL programs into
`web/pkg`; both clients fetch them from `/pkg`. The release content check
rejects missing or stale shaders and exports each runtime image's web root to
check every bundled file against the manifest-bound static bundle before writing or verifying the manifest.
The full web root (including generated packages, snippets and shaders) travels
through `dev-artifacts` into `bundle.tar` and both images; `main-promotion`
reuses those exact attested artifacts without rebuilding or selective copying.
The runtime images run as non-root users with read-only filesystems
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
registry digests. Running CI workflows on `dev` and `main` finish before the
newest pending run starts; superseded pending runs are replaced. Pull requests,
including release PRs, still cancel superseded runs; release branch pushes do
not trigger CI, and release branch dispatches retain cancellation. On a `dev`
push, `dev-artifacts` waits for the same run's `required` check, then downloads
`release-e2e-evidence` (the complete, revision-bound `pass.json` already produced
by `make coverage` in `native-coverage`) and `performance-report` (including
`perf-ci`'s `ci.json`). It reuses those reports without repeating validation;
`release-build` still rejects stale or failing evidence and dirty performance
reports. Coverage remains uploaded by `native-coverage`; the artifact job's
unused coverage and `perf-full` reruns are removed. Nightly still uploads
`perf-full` output. The job builds and verifies the local release once,
generates SPDX SBOMs for both runtime images and the static bundle,
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
and registry digests, and verifies the OIDC attestations. Image attestations
are fetched from GHCR OCI referrers; image and static-bundle SBOMs are verified
with the SPDX v2.3 predicate. It starts candidate images by digest in a
disposable stack. When a previous promoted release exists, it also starts
that release and rolls back to its exact digests. The
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
