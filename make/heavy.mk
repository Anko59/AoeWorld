# Heavy gates whose recipe is not one harness command: the steps run under the slot
# (docs/gates.md "Heavy slots"). Each target runs its `-steps` target in the orchestrator container
# through `aoe-harness slot`; the registry decides whether the gate takes a slot. The steps read
# the environment named in SLOT_ENV (the audit in slots/tests/makefile.rs checks the list).
SLOT_ENV := \
  -e BUILDING_PROOF_REQUEST \
  -e HOME \
  -e DE_DIR \
  -e HARNESS_TEST_FILTER \
  -e HARNESS_DUEL_BASE \
  -e HARNESS_DUEL_REVISION \
  -e AOE_SOURCE_QUAL_PACKAGE_DIRECTORY \
  -e AOE_SOURCE_QUAL_CONTENT_HASH \
  -e AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY \
  -e AOE_SOURCE_QUAL_GEOGRAPHIC_CONTENT_HASH \
  -e AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY \
  -e AOE_SOURCE_QUAL_512_CONTENT_HASH \
  -e AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY \
  -e AOE_SOURCE_QUAL_16384_CONTENT_HASH \
  -e AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY \
  -e AOE_SOURCE_QUAL_262144_CONTENT_HASH \
  -e AOE_ASSET_PACK \
  -e AOE_E2E_SHARD \
  -e AOE_E2E_WORKERS \
  -e AOE_E2E_SPECS \
  -e AOE_GEODATA_CACHE \
  -e AOE_MAP_REQUEST \
  -e AOE_MAP_PACKAGE \
  -e AOE_MAP_SAMPLES \
  -e AOE_MAP_DEM_RESOLUTION \
  -e AOE_MAP_HYDROLOGY_MODE \
  -e AOE_POLISH_SOURCE_URL \
  -e AOE_POLISH_SOURCE_HASH \
  -e GIT_INDEX_FILE \
  -e RUST_TEST_THREADS \
  -e NEXTEST_TEST_THREADS \
  -e AOE_SHIP_EVIDENCE \
  -e AOE_HARNESS_BOOT_GAP \
  -e TMPDIR \
  -e $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE)) \
  -e $(call MOUNT_QUOTE,HARNESS_TARGET_CACHE=$(HARNESS_TARGET_CACHE))
SLOT_RUN = \
  docker run --rm --init \
  --label aoe.harness-boot-gap=$(AOE_HARNESS_BOOT_GAP) \
  $(HEAVY_ENV) \
  $(SLOT_CAP) \
  $(SLOT_ENV) \
  --user $(UID):$(GID) \
  --group-add $(shell stat -c %g /var/run/docker.sock) \
  -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) \
  $(ROOT_MOUNTS) \
  $(if $(TMPDIR),-v $(TMPDIR):$(TMPDIR)) \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -w $(ROOT) \
  $(ORCH_IMAGE) \
  target/debug/aoe-harness slot

# The command bootstrap is static work: one build at a time across worktrees,
# bounded to one CPU and 2 GiB including swap. Gate steps still require a slot.
SLOT_BOOTSTRAP = docker run --rm --init $(HEAVY_ENV) --memory=2g --memory-swap=2g --cpus=1 --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo build --locked --jobs=1 -p aoe-harness
.PHONY: slot-bootstrap perf-wasm-cap
slot-bootstrap: orchestrator-tools
	@flock '$(GIT_COMMON)/aoe-slot-bootstrap.lock' $(SLOT_BOOTSTRAP)

export AOE_DISK_FLOOR_GIB
.PHONY: disk-precheck
disk-precheck: slot-bootstrap
	@env $(call MOUNT_QUOTE,HARNESS_TARGET_CACHE=$(HARNESS_TARGET_CACHE)) $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE)) '$(HARNESS_TARGET_CACHE)/debug/aoe-harness' disk-precheck

# Check on the host before prerequisites can take a slot, including with make -j.
preflight: disk-precheck
	@$(MAKE) --no-print-directory deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight

coverage: coverage-tools browser-deps orchestrator-tools build-wasm slot-bootstrap
	@$(SLOT_RUN) coverage -- make --no-print-directory coverage-steps
	@$(MAKE) --no-print-directory coverage-check

coverage-steps:
	@mkdir -p reports/coverage
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov nextest --locked --workspace --lcov --output-path reports/coverage/native.lcov
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e AOE_E2E_COVERAGE=1 -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-e2e-%p.profraw -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-e2e
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-wasm-%p.profraw -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-wasm
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --lcov --output-path reports/coverage/native.lcov
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --json --output-path reports/coverage/native.json

perf-instructions: analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-instructions -- make --no-print-directory perf-instructions-steps

perf-instructions-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench instructions -- --output-format=json > reports/perf/simulation.ndjson
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-protocol --bench instructions -- --output-format=json > reports/perf/protocol.ndjson
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-assets --bench instructions -- --output-format=json > reports/perf/assets.ndjson

perf-timing: tools slot-bootstrap
	@$(SLOT_RUN) perf-timing -- make --no-print-directory perf-timing-steps

perf-timing-steps:
	@$(DOCKER_RUN) cargo bench --locked -p aoe-simulation --bench timings -- --noplot --warm-up-time 0.5 --measurement-time 1 --sample-size 10
	@$(DOCKER_RUN) cargo bench --locked -p aoe-protocol --bench timings -- --noplot --warm-up-time 0.5 --measurement-time 1 --sample-size 10
	@$(DOCKER_RUN) cargo bench --locked -p aoe-assets --bench timings -- --noplot --warm-up-time 0.5 --measurement-time 1 --sample-size 10
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-timing-report

perf-wasm-size: build-wasm analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-wasm-size -- make --no-print-directory perf-wasm-size-steps

perf-wasm-size-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) wasm-opt -Oz --strip-debug web/pkg/aoe_client_bg.wasm -o reports/perf/optimized.wasm
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) gzip -n -9 -c reports/perf/optimized.wasm > reports/perf/optimized.wasm.gz
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) wasm-opt -Oz --strip-debug web/diagnostics-pkg/aoe_client_bg.wasm -o reports/perf/diagnostics-optimized.wasm
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) gzip -n -9 -c reports/perf/diagnostics-optimized.wasm > reports/perf/diagnostics-optimized.wasm.gz

# The local ship's cap check (preflight): measure (perf-wasm-size), then compare with baselines/perf/wasm.json
# under the gate's own rule. It measures in its own gate snapshot: each gate runs in a separate checkout, so it
# cannot read another gate's artifact. It is heavy, so its slot and load wait come before its deadline, and the
# nested perf-wasm-size inherits that slot instead of waiting again. perf-wasm-size alone stays a measurement.
perf-wasm-cap: perf-wasm-size
	@target/debug/aoe-harness perf-wasm-size-check

build-wasm: slot-bootstrap
	@$(SLOT_RUN) build-wasm -- make --no-print-directory build-wasm-steps

build-wasm-steps:
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features diagnostics
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/diagnostics-pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm

test-e2e: disk-precheck slot-bootstrap
	@$(MAKE) --no-print-directory build-wasm browser-deps orchestrator-tools
	@$(SLOT_RUN) test-e2e -- make --no-print-directory test-e2e-steps

test-e2e-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -e AOE_ASSET_PACK -e AOE_E2E_SHARD -e AOE_E2E_WORKERS -e AOE_E2E_SPECS -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-e2e

.PHONY: \
  test-harness \
  test-harness-steps \
  test-unit-steps \
  test-wasm-steps \
  test-source-qualification \
  test-source-qualification-steps \
  test-exhaustive \
  test-exhaustive-steps
test-harness: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-harness -- make --no-print-directory test-harness-steps

test-harness-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness --no-fail-fast

test-unit-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-unit

.PHONY: test-assets-de test-assets-de-steps
test-assets-de: slot-bootstrap
	@$(SLOT_RUN) test-assets-de -- make --no-print-directory test-assets-de-steps

test-assets-de-steps:
	@$(subst $(TOOL_IMAGE),-e AOE_DE_DIR=/de-install $(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo nextest run --locked -p aoe-assets --no-tests fail --success-output immediate -E 'test(real_house)'

.PHONY: test-assets-dat test-assets-dat-steps
test-assets-dat: slot-bootstrap
	@$(SLOT_RUN) test-assets-dat -- make --no-print-directory test-assets-dat-steps

test-assets-dat-steps:
	@$(subst $(TOOL_IMAGE),-e AOE_TRIAL_DAT=$(ROOT)/local-assets/trial/Data/empires2.dat $(TOOL_IMAGE),$(DOCKER_RUN)) cargo nextest run --locked -p aoe-assets --no-tests fail --success-output immediate -E 'test(dat::)'

test-source-qualification: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-source-qualification -- make --no-print-directory test-source-qualification-steps

test-source-qualification-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-source-qualification

test-exhaustive: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-exhaustive -- make --no-print-directory test-exhaustive-steps

test-exhaustive-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-exhaustive

test-wasm-steps:
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-wasm

# Heavy work outside the registry: each target takes a slot under its own name, so `aoe-harness slot`
# queues it like a gate. The steps run inside the slot; nested calls inherit it.
build: slot-bootstrap
	@$(SLOT_RUN) build -- make --no-print-directory build-steps

build-steps:
	@$(DOCKER_RUN) cargo build --workspace --locked

map-test: slot-bootstrap
	@$(SLOT_RUN) map-test -- make --no-print-directory map-test-steps

map-test-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-map

geodata-test: slot-bootstrap
	@$(SLOT_RUN) geodata-test -- make --no-print-directory geodata-test-steps

geodata-test-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-geodata

test-geographic-visuals-unit: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-geographic-visuals-unit -- make --no-print-directory test-geographic-visuals-unit-steps

test-geographic-visuals-unit-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness e2e::visual::packages
	@$(DOCKER_RUN) cargo test --locked -p aoe-server explicit_map_package_directory_is_canonical_and_must_be_a_directory

test-placement-coverage: coverage-tools slot-bootstrap
	@$(SLOT_RUN) test-placement-coverage -- make --no-print-directory test-placement-coverage-steps

test-placement-coverage-steps:
	@mkdir -p reports/placement
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov nextest --locked -p aoe-harness --no-report -E 'test(gates::placement::tests::)'
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov run --locked -p aoe-harness --no-report -- test-placement
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --lcov --output-path reports/placement/coverage.lcov

test-harness-focused: slot-bootstrap
	@$(SLOT_RUN) test-harness-focused -- make --no-print-directory test-harness-focused-steps

test-harness-focused-steps:
	@test -n "$$HARNESS_TEST_FILTER"
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness "$$HARNESS_TEST_FILTER"

duel-check: slot-bootstrap
	@$(SLOT_RUN) duel-check -- make --no-print-directory duel-check-steps

duel-check-steps:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GIT_INDEX_FILE -e AOE_SHIP_EVIDENCE -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- duel-check
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION -e AOE_ASSET_PACK $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- duel-browser-check

test-country-source: build-wasm browser-deps orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-country-source -- make --no-print-directory test-country-source-steps

test-country-source-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_ASSET_PACK $(ROOT_MOUNTS) $(SOURCE_QUAL_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' --browser

dev-build: slot-bootstrap
	@$(SLOT_RUN) dev-build -- make --no-print-directory dev-build-steps

dev-build-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker

# Single-command and release-compile targets: the slot is taken before the first container, so
# every container they start is capped (the audit in slots/tests/makefile.rs lists the exemptions).
fuzz-smoke: fuzz-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) fuzz-smoke -- make --no-print-directory fuzz-smoke-steps

fuzz-smoke-steps:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-smoke

fuzz-nightly: fuzz-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) fuzz-nightly -- make --no-print-directory fuzz-nightly-steps

fuzz-nightly-steps:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-nightly

mutation-nightly: mutation-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) mutation-nightly -- make --no-print-directory mutation-nightly-steps

mutation-nightly-steps:
	@mkdir -p reports/mutation
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) --tmpfs /tmp:rw,exec,size=4g -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e CARGO_TARGET_DIR=$(ROOT)/.cache/mutation/driver-target $(ROOT_MOUNTS) -w $(ROOT) $(MUTATION_IMAGE) cargo run --locked -p aoe-harness -- mutation-nightly

perf-smoke: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-smoke -- make --no-print-directory perf-smoke-steps

perf-smoke-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-smoke

perf-ci: perf-instructions perf-wasm-size orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-ci -- make --no-print-directory perf-ci-steps

perf-ci-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-ci

perf-full: perf-instructions perf-wasm-size orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-full -- make --no-print-directory perf-full-steps

perf-full-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-full

perf-pressure: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-pressure -- make --no-print-directory perf-pressure-steps

perf-pressure-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-pressure

perf-stress: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-stress -- make --no-print-directory perf-stress-steps

perf-stress-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-stress

perf-soak-10: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-soak-10 -- make --no-print-directory perf-soak-10-steps

perf-soak-10-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-soak-10

perf-soak-30: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-soak-30 -- make --no-print-directory perf-soak-30-steps

perf-soak-30-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-soak-30

# Map, perf and fuzz targets, one per line so two branches adding different ones do not conflict.
.PHONY: \
  geodata-bootstrap \
  geodata-verify \
  map-estimate \
  map-generate \
  map-generate-detailed \
  map-verify \
  map-test \
  map-perf \
  map-source-qualify \
  map-source-scale-qualify
.PHONY: \
  fuzz-smoke \
  fuzz-nightly \
  mutation-nightly \
  perf-smoke \
  perf-ci \
  perf-full \
  perf-pressure \
  perf-stress \
  perf-soak-10 \
  perf-soak-30 \
  perf-instructions \
  perf-timing \
  perf-wasm-size \
  perf-baseline-propose \
  perf-comment \
  perf-hardware-check
