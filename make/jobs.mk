# Jobs run on the host before any workload container. Only long jobs lock.
JOB_BINARY := $(HARNESS_TARGET_CACHE)/debug/aoe-harness
JOB_TARGET := $(subst :,\:,$(JOB_BINARY))
JOB_SOURCES := $(shell rg --files crates -g '*.rs' -g Cargo.toml) Cargo.toml Cargo.lock
JOB_RUN = env $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE)) $(call MOUNT_QUOTE,HARNESS_TARGET_CACHE=$(HARNESS_TARGET_CACHE)) '$(JOB_BINARY)' job
.PHONY: job-bootstrap perf-wasm-cap
job-bootstrap: $(JOB_TARGET)

$(JOB_TARGET): .EXTRA_PREREQS :=
$(JOB_TARGET): $(JOB_SOURCES) | resource-bootstrap cache-mount-points
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@touch $(call MOUNT_QUOTE,$(JOB_BINARY))

export AOE_DISK_FLOOR_GIB
.PHONY: disk-precheck
disk-precheck: job-bootstrap
	@env $(call MOUNT_QUOTE,HARNESS_TARGET_CACHE=$(HARNESS_TARGET_CACHE)) $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE)) '$(HARNESS_TARGET_CACHE)/debug/aoe-harness' disk-precheck

# Admit standalone preflight before its workload prerequisites, including with make -j.
preflight: resource-bootstrap
	@$(RESOURCE_LONG) preflight -- $(MAKE) --no-print-directory preflight-admitted

.PHONY: preflight-admitted
preflight-admitted: job-bootstrap
	@$(MAKE) --no-print-directory preflight-steps

preflight-steps: disk-precheck
	@$(MAKE) --no-print-directory deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight

coverage: resource-bootstrap
	@$(RESOURCE_LONG) coverage -- $(MAKE) --no-print-directory coverage-admitted

.PHONY: coverage-admitted
coverage-admitted: job-bootstrap
	@$(MAKE) --no-print-directory coverage-steps
	@$(MAKE) --no-print-directory coverage-check

coverage-steps:
	@$(MAKE) --no-print-directory coverage-tools browser-deps orchestrator-tools build-wasm
	@mkdir -p reports/coverage
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) -e CARGO_BUILD_JOBS=2 --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) $(RESOURCE_OBSERVER) cargo llvm-cov nextest --locked --workspace --lcov --output-path reports/coverage/native.lcov
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e AOE_E2E_COVERAGE=1 -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-e2e-%p.profraw -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) target/llvm-cov-target/debug/aoe-harness test-e2e
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-wasm-%p.profraw -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) target/llvm-cov-target/debug/aoe-harness test-wasm
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) $(RESOURCE_OBSERVER) cargo llvm-cov report --lcov --output-path reports/coverage/native.lcov
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) $(RESOURCE_OBSERVER) cargo llvm-cov report --json --output-path reports/coverage/native.json

perf-instructions: resource-bootstrap
	@$(RESOURCE_LONG) perf-instructions -- $(MAKE) --no-print-directory perf-instructions-admitted

.PHONY: perf-instructions-admitted
perf-instructions-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-instructions-steps

perf-instructions-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-simulation --bench instructions -- --output-format=json > reports/perf/simulation.ndjson
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-protocol --bench instructions -- --output-format=json > reports/perf/protocol.ndjson
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-assets --bench instructions -- --output-format=json > reports/perf/assets.ndjson

# Headless 100-player, 100,000-unit tick benchmark; PERF_SCALE_ARGS passes harness flags
# (for example `PERF_SCALE_ARGS="--players 8 --ticks 100" make perf-scale`).
perf-scale: resource-bootstrap
	@$(RESOURCE_LONG) perf-scale -- $(MAKE) --no-print-directory perf-scale-admitted

.PHONY: perf-scale-admitted
perf-scale-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-scale-steps

perf-scale-steps:
	@$(MAKE) --no-print-directory tools
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-scale $(PERF_SCALE_ARGS)

# A tiny run (4 players, 2,000 units, 50 ticks) that executes the whole benchmark path.
perf-scale-smoke: resource-bootstrap
	@$(RESOURCE_LONG) perf-scale-smoke -- $(MAKE) --no-print-directory perf-scale-smoke-admitted

.PHONY: perf-scale-smoke-admitted
perf-scale-smoke-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-scale-smoke-steps

perf-scale-smoke-steps:
	@$(MAKE) --no-print-directory tools
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-scale --players 4 --units-per-player 500 --side-tiles 4000 --ticks 50 --fog-off-ticks 10 --server-ticks 5 --subscribers 4 --out reports/perf/perf-scale-smoke.json

# About 11,000 military movers in 200 armies on open ground: the movement span split
# (docs/movement/group-movement-rewrite.md section 5.2). PERF_GROUPS_ARGS passes harness flags.
perf-groups: resource-bootstrap
	@$(RESOURCE_LONG) perf-groups -- $(MAKE) --no-print-directory perf-groups-admitted

.PHONY: perf-groups-admitted
perf-groups-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-groups-steps

perf-groups-steps:
	@$(MAKE) --no-print-directory tools
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-groups $(PERF_GROUPS_ARGS)

# Per-function Binaryen metrics retain the names in wasm-bindgen's output.
.PHONY: perf-wasm-functions perf-wasm-functions-steps
perf-wasm-functions: build-wasm analysis-tools job-bootstrap
	@$(JOB_RUN) perf-wasm-functions -- make --no-print-directory perf-wasm-functions-steps

perf-wasm-functions-steps:
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) wasm-opt web/pkg/aoe_client_bg.wasm --func-metrics -o /dev/null > reports/perf/wasm-functions.txt

perf-wasm-size: build-wasm analysis-tools job-bootstrap
	@$(JOB_RUN) perf-wasm-size -- make --no-print-directory perf-wasm-size-steps

perf-wasm-size-steps:
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) wasm-opt -Oz --strip-debug web/pkg/aoe_client_bg.wasm -o reports/perf/optimized.wasm
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) gzip -n -9 -c reports/perf/optimized.wasm > reports/perf/optimized.wasm.gz
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) wasm-opt -Oz --strip-debug web/diagnostics-pkg/aoe_client_bg.wasm -o reports/perf/diagnostics-optimized.wasm
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) gzip -n -9 -c reports/perf/diagnostics-optimized.wasm > reports/perf/diagnostics-optimized.wasm.gz

# The local ship's cap check (preflight): measure (perf-wasm-size), then compare with baselines/perf/wasm.json
# under the gate's own rule. It measures in its own gate snapshot: each gate runs in a separate checkout, so it
# cannot read another gate's artifact. Size analysis is short and uses the normal container caps and
# immediate memory admission; perf-wasm-size alone stays a measurement.
perf-wasm-cap: perf-wasm-size
	@target/debug/aoe-harness perf-wasm-size-check

build-wasm: job-bootstrap
	@$(JOB_RUN) build-wasm -- make --no-print-directory build-wasm-steps

build-wasm-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets browser-shaders
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features diagnostics
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/diagnostics-pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm

test-e2e: resource-bootstrap
	@$(RESOURCE_LONG) test-e2e -- $(MAKE) --no-print-directory test-e2e-admitted

.PHONY: test-e2e-admitted
test-e2e-admitted: job-bootstrap
	@$(MAKE) --no-print-directory test-e2e-steps

test-e2e-steps:
	@$(MAKE) --no-print-directory disk-precheck build-wasm browser-deps orchestrator-tools
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -e AOE_ASSET_PACK -e AOE_E2E_SHARD -e AOE_E2E_WORKERS -e AOE_E2E_PORT_RANGE -e AOE_E2E_SPECS -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- test-e2e

# Keep the focused memory investigation serial within this command, including nested builds.
# Every child keeps its normal resource admission and checks.
.PHONY: test-client-memory
test-client-memory: export AOE_E2E_WORKERS := 1
test-client-memory: disk-precheck job-bootstrap
	$(if $(strip $(AOE_E2E_SPECS)),,$(error test-client-memory requires focused AOE_E2E_SPECS))
	@$(JOB_RUN) test-client-memory -- make --no-print-directory -j1 test-unit test-wasm test-e2e

.PHONY: \
  test-harness \
  test-harness-steps \
  test-unit-steps \
  test-wasm-steps \
  test-source-qualification \
  test-source-qualification-steps \
  test-exhaustive \
  test-exhaustive-steps
test-harness: orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-harness -- make --no-print-directory test-harness-steps

test-harness-steps:
	# Harness fixtures mutate process-global environment and must not race other tests.
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness --no-fail-fast -- --test-threads=1

test-unit-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-unit

# Focused iteration; the full test-unit gate remains unchanged.
.PHONY: test-game-focused test-game-focused-steps
test-game-focused: job-bootstrap
	@$(JOB_RUN) test-game-focused -- make --no-print-directory test-game-focused-steps

test-game-focused-steps:
	@$(DOCKER_RUN) cargo nextest run --locked -p aoe-core -p aoe-assets -p aoe-simulation --no-tests fail --no-fail-fast -E $(call MOUNT_QUOTE,$(AOE_GAME_TEST_FILTER))

.PHONY: test-assets-de test-assets-de-steps
test-assets-de: job-bootstrap
	@$(JOB_RUN) test-assets-de -- make --no-print-directory test-assets-de-steps

test-assets-de-steps:
	@$(subst $(TOOL_IMAGE),-e AOE_DE_DIR=/de-install $(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo nextest run --locked -p aoe-assets --no-tests fail --success-output immediate -E 'test(real_house)|test(real_siege_stone)|test(real_galley_bolt)|test(real_all_unit)|test(real_de_building)|test(real_terrain_directory)'

.PHONY: test-assets-dat test-assets-dat-steps
test-assets-dat: job-bootstrap
	@$(JOB_RUN) test-assets-dat -- make --no-print-directory test-assets-dat-steps

test-assets-dat-steps:
	@$(subst $(TOOL_IMAGE),-e AOE_TRIAL_DAT=$(ROOT)/local-assets/trial/Data/empires2.dat $(TOOL_IMAGE),$(DOCKER_RUN)) cargo nextest run --locked -p aoe-assets --no-tests fail --success-output immediate -E 'test(dat::)'

test-source-qualification: orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-source-qualification -- make --no-print-directory test-source-qualification-steps

test-source-qualification-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-source-qualification

test-exhaustive: orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-exhaustive -- make --no-print-directory test-exhaustive-steps

test-exhaustive-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-exhaustive

test-wasm-steps:
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -e AOE_E2E_PORT_RANGE -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- test-wasm

.PHONY: test-wasm-focused test-wasm-focused-steps
test-wasm-focused: disk-precheck browser-tools orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-wasm-focused -- make --no-print-directory test-wasm-focused-steps

test-wasm-focused-steps:
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_WASM_TEST_PACKAGE -e AOE_WASM_TEST_FILTER $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -e AOE_E2E_PORT_RANGE -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- test-wasm-focused

# Build and test targets use the same host job wrapper; short work never takes a lock.
build: job-bootstrap
	@$(JOB_RUN) build -- make --no-print-directory build-steps

build-steps:
	@$(DOCKER_RUN) cargo build --workspace --locked

map-test: job-bootstrap
	@$(JOB_RUN) map-test -- make --no-print-directory map-test-steps

map-test-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-map

geodata-test: job-bootstrap
	@$(JOB_RUN) geodata-test -- make --no-print-directory geodata-test-steps

geodata-test-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-geodata

test-geographic-visuals-unit: orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-geographic-visuals-unit -- make --no-print-directory test-geographic-visuals-unit-steps

test-geographic-visuals-unit-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness e2e::visual::packages
	@$(DOCKER_RUN) cargo test --locked -p aoe-server explicit_map_package_directory_is_canonical_and_must_be_a_directory

.PHONY: test-native-focused test-native-focused-steps
# A developer-selected Nextest filter keeps iteration under the normal resource caps.
test-native-focused: job-bootstrap
	@$(JOB_RUN) test-native-focused -- make --no-print-directory test-native-focused-steps

test-native-focused-steps:
	@test -n "$$AOE_NATIVE_TEST_FILTER"
	@test -n "$$AOE_NATIVE_TEST_PACKAGE"
	@$(DOCKER_RUN) cargo nextest run --locked -p "$$AOE_NATIVE_TEST_PACKAGE" --no-tests fail --no-fail-fast -E "$$AOE_NATIVE_TEST_FILTER"

test-harness-focused: job-bootstrap
	@$(JOB_RUN) test-harness-focused -- make --no-print-directory test-harness-focused-steps

test-harness-focused-steps:
	@test -n "$$HARNESS_TEST_FILTER"
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness "$$HARNESS_TEST_FILTER"

duel-check: job-bootstrap
	@$(JOB_RUN) duel-check -- make --no-print-directory duel-check-steps

duel-check-steps:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GIT_INDEX_FILE -e AOE_SHIP_EVIDENCE -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- duel-check
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION -e AOE_ASSET_PACK $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- duel-browser-check

test-country-source: build-wasm browser-deps orchestrator-tools job-bootstrap
	@$(JOB_RUN) test-country-source -- make --no-print-directory test-country-source-steps

test-country-source-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_ASSET_PACK $(ROOT_MOUNTS) $(SOURCE_QUAL_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' --browser

dev-build: job-bootstrap sdk-python-tools
	@$(JOB_RUN) dev-build -- make --no-print-directory dev-build-steps

dev-build-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@make --no-print-directory sdk-python-steps

# Single-command and release-compile targets use the same container caps and admission.
# resources/tests/makefile.rs audits the launch boundaries.
fuzz-smoke: fuzz-tools orchestrator-tools job-bootstrap
	@$(JOB_RUN) fuzz-smoke -- make --no-print-directory fuzz-smoke-steps

fuzz-smoke-steps:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-smoke

fuzz-nightly: fuzz-tools orchestrator-tools job-bootstrap
	@$(JOB_RUN) fuzz-nightly -- make --no-print-directory fuzz-nightly-steps

fuzz-nightly-steps:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-nightly

perf-smoke: resource-bootstrap
	@$(RESOURCE_LONG) perf-smoke -- $(MAKE) --no-print-directory perf-smoke-admitted

.PHONY: perf-smoke-admitted
perf-smoke-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-smoke-steps

perf-smoke-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-smoke

# Standalone, identical late-game repetitions for before/after profiling.
.PHONY: perf-late-game perf-late-game-steps
perf-late-game: resource-bootstrap
	@$(RESOURCE_LONG) perf-late-game -- $(MAKE) --no-print-directory perf-late-game-admitted

.PHONY: perf-late-game-admitted
perf-late-game-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-late-game-steps

perf-late-game-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
	@$(RESOURCE_RUN) docker run --rm --init $(JOB_ENV) $(RESOURCE_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) $(RESOURCE_OBSERVER) cargo run --release --locked -p aoe-harness -- perf-late-game $(AOE_LATE_GAME_ARGS)

perf-ci: resource-bootstrap
	@$(RESOURCE_LONG) perf-ci -- $(MAKE) --no-print-directory perf-ci-admitted

.PHONY: perf-ci-admitted
perf-ci-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-ci-steps

perf-ci-steps:
	@$(MAKE) --no-print-directory analysis-tools orchestrator-tools
	@make --no-print-directory perf-wasm-size perf-instructions
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-ci

perf-full: resource-bootstrap
	@$(RESOURCE_LONG) perf-full -- $(MAKE) --no-print-directory perf-full-admitted

.PHONY: perf-full-admitted
perf-full-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-full-steps

perf-full-steps:
	@$(MAKE) --no-print-directory perf-instructions perf-wasm-size orchestrator-tools
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-full

perf-pressure: resource-bootstrap
	@$(RESOURCE_LONG) perf-pressure -- $(MAKE) --no-print-directory perf-pressure-admitted

.PHONY: perf-pressure-admitted
perf-pressure-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-pressure-steps

perf-pressure-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-pressure

perf-stress: resource-bootstrap
	@$(RESOURCE_LONG) perf-stress -- $(MAKE) --no-print-directory perf-stress-admitted

.PHONY: perf-stress-admitted
perf-stress-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-stress-steps

perf-stress-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-stress

perf-soak-10: resource-bootstrap
	@$(RESOURCE_LONG) perf-soak-10 -- $(MAKE) --no-print-directory perf-soak-10-admitted

.PHONY: perf-soak-10-admitted
perf-soak-10-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-soak-10-steps

perf-soak-10-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-soak-10

perf-soak-30: resource-bootstrap
	@$(RESOURCE_LONG) perf-soak-30 -- $(MAKE) --no-print-directory perf-soak-30-admitted

.PHONY: perf-soak-30-admitted
perf-soak-30-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-soak-30-steps

perf-soak-30-steps:
	@$(MAKE) --no-print-directory orchestrator-tools
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
  perf-smoke \
  perf-ci \
  perf-full \
  perf-pressure \
  perf-stress \
  perf-soak-10 \
  perf-soak-30 \
  perf-instructions \
  perf-scale \
  perf-scale-smoke \
  perf-groups \
  perf-wasm-size \
  perf-baseline-propose \
  perf-comment \
  perf-dev-reference \
  perf-hardware-check

.PHONY: test-navigation test-navigation-steps
.PHONY: test-auto-scout test-auto-scout-steps
test-auto-scout: disk-precheck job-bootstrap
	@$(JOB_RUN) test-auto-scout -- make --no-print-directory test-auto-scout-steps

test-auto-scout-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation --lib game_world::economy::scouting::tests -- --show-output

.PHONY: test-economy-automation test-economy-automation-steps
test-economy-automation: disk-precheck job-bootstrap
	@$(JOB_RUN) test-economy-automation -- make --no-print-directory test-economy-automation-steps

test-economy-automation-steps: disk-precheck
	@$(DOCKER_RUN) cargo nextest run --locked -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-server -p aoe-harness --no-fail-fast --no-tests fail --failure-output final -E 'package(aoe-protocol) | package(aoe-sdk) | test(gathering) | test(farming) | test(scouting) | test(garrison) | test(pinned) | test(per_session_streams_cross_views_without_leaking_unseen_state)'

test-navigation: job-bootstrap
	@$(JOB_RUN) test-navigation -- make --no-print-directory test-navigation-steps

test-navigation-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-map navigation::hierarchy::prepared::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-map navigation::hierarchy::static_data::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation game_cache::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-map navigation::hierarchy::tests::static_data
	@$(DOCKER_RUN) cargo test --locked -p aoe-map pocket
	@$(DOCKER_RUN) cargo test --locked -p aoe-server map_store::navigation::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-server map_store::navigation::shared::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-server gameplay::probe::tests
	@$(DOCKER_RUN) cargo test --locked -p aoe-server complete_static_navigation_never_queues_cold_move_preparation
	@$(DOCKER_RUN) cargo test --locked -p aoe-server a_job_for_a_stored_preset_publishes_navigation_without_a_worker
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation pinned

.PHONY: test-navigation-perf test-navigation-perf-steps
test-navigation-perf: job-bootstrap
	@$(JOB_RUN) test-navigation-perf -- make --no-print-directory test-navigation-perf-steps

test-navigation-perf-steps:
	@$(DOCKER_RUN) cargo test --release --locked -p aoe-server --lib large_navigation_ -- --nocapture --test-threads=1

.PHONY: test-bridges test-bridges-steps
test-bridges: job-bootstrap
	@$(JOB_RUN) test-bridges -- make --no-print-directory test-bridges-steps

test-bridges-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-map -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-assets bridge
.PHONY: test-formations test-formations-steps
test-formations: job-bootstrap
	@$(JOB_RUN) test-formations -- make --no-print-directory test-formations-steps

test-formations-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-server --lib formation -- --nocapture
	@$(DOCKER_RUN) cargo test --locked -p aoe-sdk --lib

.PHONY: test-formations-perf test-formations-perf-steps
test-formations-perf: job-bootstrap
	@$(JOB_RUN) test-formations-perf -- make --no-print-directory test-formations-perf-steps

test-formations-perf-steps:
	@$(DOCKER_RUN) cargo test --locked --release -p aoe-simulation --lib formations_measure_warm -- --nocapture
# One complete late-game tick under Callgrind for focused optimisation work.
.PHONY: perf-late-game-instructions perf-late-game-instructions-steps
perf-late-game-instructions: resource-bootstrap
	@$(RESOURCE_LONG) perf-late-game-instructions -- $(MAKE) --no-print-directory perf-late-game-instructions-admitted

.PHONY: perf-late-game-instructions-admitted
perf-late-game-instructions-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-late-game-instructions-steps

perf-late-game-instructions-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-simulation --bench late_game_instructions -- --output-format=json > reports/perf/late-game-instructions.ndjson
