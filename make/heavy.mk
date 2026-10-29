# Heavy gates whose recipe is not one harness command: the steps run under the slot
# (docs/gates.md "Heavy slots"). Each target runs its `-steps` target in the orchestrator container
# through `aoe-harness slot`; the registry decides whether the gate takes a slot. The steps read
# the environment named in SLOT_ENV (the audit in slots/tests/makefile.rs checks the list).
SLOT_ENV := \
  -e BUILDING_PROOF_REQUEST \
  -e HOME \
  -e DE_DIR \
  -e HARNESS_TEST_FILTER \
  -e PERF_GROUPS_ARGS \
  -e CARGO_INCREMENTAL \
  -e CARGO_PROFILE_DEV_DEBUG \
  -e CARGO_PROFILE_TEST_DEBUG \
  -e AOE_GAME_TEST_FILTER \
  -e AOE_UI_PROOF_PROJECT \
  -e AOE_NATIVE_TEST_FILTER \
  -e AOE_NATIVE_TEST_PACKAGE \
  -e AOE_WASM_TEST_FILTER \
  -e AOE_WASM_TEST_PACKAGE \
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
  -e AOE_LATE_GAME_ARGS \
  -e PERF_SCALE_ARGS \
  -e PERF_GROUPS_ARGS \
  -e SCALE_MATCH_ARGS \
  -e AOE_ARENA \
  -e AOE_SCENARIO \
  -e AOE_OPEN_MAP_PACKAGE \
  -e AOE_FOG_DISABLED \
  -e AOE_FOG_SHOWCASE \
  -e AOE_MATCH_TOKEN \
  -e AOE_PRESET_MAPS \
  -e DEV_PORT \
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

# Headless 100-player, 100,000-unit tick benchmark; PERF_SCALE_ARGS passes harness flags
# (for example `PERF_SCALE_ARGS="--players 8 --ticks 100" make perf-scale`).
perf-scale: tools slot-bootstrap
	@$(SLOT_RUN) perf-scale -- make --no-print-directory perf-scale-steps

perf-scale-steps:
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-scale $(PERF_SCALE_ARGS)

# A tiny run (4 players, 2,000 units, 50 ticks) that executes the whole benchmark path.
perf-scale-smoke: tools slot-bootstrap
	@$(SLOT_RUN) perf-scale-smoke -- make --no-print-directory perf-scale-smoke-steps

perf-scale-smoke-steps:
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-scale --players 4 --units-per-player 500 --side-tiles 4000 --ticks 50 --fog-off-ticks 10 --server-ticks 5 --subscribers 4 --out reports/perf/perf-scale-smoke.json

# About 11,000 military movers in 200 armies on open ground: the movement span split
# (docs/movement/group-movement-rewrite.md section 5.2). PERF_GROUPS_ARGS passes harness flags.
perf-groups: tools slot-bootstrap
	@$(SLOT_RUN) perf-groups -- make --no-print-directory perf-groups-steps

perf-groups-steps:
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-groups $(PERF_GROUPS_ARGS)

# Per-function Binaryen metrics retain the names in wasm-bindgen's output.
.PHONY: perf-wasm-functions perf-wasm-functions-steps
perf-wasm-functions: build-wasm analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-wasm-functions -- make --no-print-directory perf-wasm-functions-steps

perf-wasm-functions-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) wasm-opt web/pkg/aoe_client_bg.wasm --func-metrics -o /dev/null > reports/perf/wasm-functions.txt

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
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets browser-shaders
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

# Keep the memory investigation serial, including its nested builds. Existing
# guards validate the inherited slot; every gate keeps its normal checks.
.PHONY: test-client-memory
test-client-memory: export AOE_E2E_WORKERS := 1
test-client-memory: disk-precheck slot-bootstrap
	$(if $(strip $(AOE_E2E_SPECS)),,$(error test-client-memory requires focused AOE_E2E_SPECS))
	@$(SLOT_RUN) test-client-memory -- make --no-print-directory -j1 test-unit test-wasm test-e2e

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

# Focused iteration; the full test-unit gate remains unchanged.
.PHONY: test-game-focused test-game-focused-steps
test-game-focused: slot-bootstrap
	@$(SLOT_RUN) test-game-focused -- make --no-print-directory test-game-focused-steps

test-game-focused-steps:
	@$(DOCKER_RUN) cargo nextest run --locked -p aoe-core -p aoe-assets -p aoe-simulation --no-tests fail --no-fail-fast -E $(call MOUNT_QUOTE,$(AOE_GAME_TEST_FILTER))

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

.PHONY: test-wasm-focused test-wasm-focused-steps
test-wasm-focused: disk-precheck browser-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-wasm-focused -- make --no-print-directory test-wasm-focused-steps

test-wasm-focused-steps:
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_WASM_TEST_PACKAGE -e AOE_WASM_TEST_FILTER $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-wasm-focused

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

.PHONY: test-native-focused test-native-focused-steps
# A developer-selected Nextest filter keeps iteration inside the shared heavy slot.
test-native-focused: slot-bootstrap
	@$(SLOT_RUN) test-native-focused -- make --no-print-directory test-native-focused-steps

test-native-focused-steps:
	@test -n "$$AOE_NATIVE_TEST_FILTER"
	@test -n "$$AOE_NATIVE_TEST_PACKAGE"
	@$(DOCKER_RUN) cargo nextest run --locked -p "$$AOE_NATIVE_TEST_PACKAGE" --no-tests fail --no-fail-fast -E "$$AOE_NATIVE_TEST_FILTER"

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

dev-build: slot-bootstrap sdk-python-tools
	@$(SLOT_RUN) dev-build -- make --no-print-directory dev-build-steps

dev-build-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@make --no-print-directory sdk-python-steps

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

perf-smoke: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-smoke -- make --no-print-directory perf-smoke-steps

perf-smoke-steps:
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-smoke

# Standalone, identical late-game repetitions for before/after profiling.
.PHONY: perf-late-game perf-late-game-steps
perf-late-game: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-late-game -- make --no-print-directory perf-late-game-steps

perf-late-game-steps:
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --release --locked -p aoe-harness -- perf-late-game $(AOE_LATE_GAME_ARGS)

perf-ci: analysis-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-ci -- make --no-print-directory perf-ci-steps

perf-ci-steps:
	@make --no-print-directory perf-wasm-size perf-instructions
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-ci

perf-full: perf-instructions perf-wasm-size orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) perf-full -- make --no-print-directory perf-full-steps

perf-full-steps:
	@$(DOCKER_RUN) cargo run --release --locked -p aoe-harness -- perf-full

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
  perf-hardware-check

.PHONY: test-navigation test-navigation-steps
.PHONY: test-auto-scout test-auto-scout-steps
test-auto-scout: disk-precheck slot-bootstrap
	@$(SLOT_RUN) test-auto-scout -- make --no-print-directory test-auto-scout-steps

test-auto-scout-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation --lib game_world::economy::scouting::tests -- --show-output

.PHONY: test-economy-automation test-economy-automation-steps
test-economy-automation: disk-precheck slot-bootstrap
	@$(SLOT_RUN) test-economy-automation -- make --no-print-directory test-economy-automation-steps

test-economy-automation-steps: disk-precheck
	@$(DOCKER_RUN) cargo nextest run --locked -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-server -p aoe-harness --no-fail-fast --no-tests fail --failure-output final -E 'package(aoe-protocol) | package(aoe-sdk) | test(gathering) | test(farming) | test(scouting) | test(garrison) | test(pinned) | test(per_session_messages_match_the_pinned_stream)'

test-navigation: slot-bootstrap
	@$(SLOT_RUN) test-navigation -- make --no-print-directory test-navigation-steps

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
test-navigation-perf: slot-bootstrap
	@$(SLOT_RUN) test-navigation-perf -- make --no-print-directory test-navigation-perf-steps

test-navigation-perf-steps:
	@$(DOCKER_RUN) cargo test --release --locked -p aoe-server --lib large_navigation_ -- --nocapture --test-threads=1

.PHONY: test-bridges test-bridges-steps
test-bridges: slot-bootstrap
	@$(SLOT_RUN) test-bridges -- make --no-print-directory test-bridges-steps

test-bridges-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-map -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-assets bridge
.PHONY: test-formations test-formations-steps
test-formations: slot-bootstrap
	@$(SLOT_RUN) test-formations -- make --no-print-directory test-formations-steps

test-formations-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-simulation -p aoe-protocol -p aoe-sdk -p aoe-server --lib formation -- --nocapture
	@$(DOCKER_RUN) cargo test --locked -p aoe-sdk --lib

.PHONY: test-formations-perf test-formations-perf-steps
test-formations-perf: slot-bootstrap
	@$(SLOT_RUN) test-formations-perf -- make --no-print-directory test-formations-perf-steps

test-formations-perf-steps:
	@$(DOCKER_RUN) cargo test --locked --release -p aoe-simulation --lib formations_measure_warm -- --nocapture
# One complete late-game tick under Callgrind for focused optimisation work.
.PHONY: perf-late-game-instructions perf-late-game-instructions-steps
perf-late-game-instructions: analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-late-game-instructions -- make --no-print-directory perf-late-game-instructions-steps

perf-late-game-instructions-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench late_game_instructions -- --output-format=json > reports/perf/late-game-instructions.ndjson
