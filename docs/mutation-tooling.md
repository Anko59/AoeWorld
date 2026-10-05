# Mutation-image native test prerequisite

The mutation image installs cargo-nextest **0.9.144** using the same release URL
and SHA-256 as the normal Rust tools image. Cargo-mutants **27.1.0**, its checksum,
the pinned Rust base, scanner arguments, selected files, mutant and command
budgets, evaluated-count floor, coverage gates and hooks remain unchanged.

The harness baseline contains actual compiled native and QA integrity canaries.
They invoke the fixed Nextest runner inside disposable Git fixtures; a mutation
image containing cargo-mutants but not Nextest cannot run that baseline correctly.
Do not skip these tests or classify the missing executable as a caught mutant.

Validate the mutation image itself, not only a different image that already has
Nextest: build `make mutation-tools`, check both tool versions, then run the
locked harness package tests in that image. A fresh temporary Cargo target and
offline dependency use test runtime availability without changing source or lock
files. These package tests include the actual baseline/mutant/working-control
canaries; their asserted failing mutant leg is not a failing package baseline.

This is a tooling dependency repair. It does not qualify an immutable mutation
campaign, bind raw outputs to a fresh execution, establish compiled-cache
provenance, run at least 30 critical mutants or authenticate an independent judge.
Those require separate retained Snapshot/source/private-Git endpoint checks,
fresh disjoint artifacts and the actual unchanged critical campaign.

Release checksum verification protects against an unexpected downloaded archive,
not a hostile local Docker controller. The coding UID controls that daemon; local
image labels, ownership, hashes and tool-version strings do not create independent
authority. Original game assets and dedicated hardware remain unqualified.
