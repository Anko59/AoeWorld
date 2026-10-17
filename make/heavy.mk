# Heavy gates whose recipe is not one harness command: the steps run under the slot
# (docs/gates.md "Heavy slots"). Each target runs its `-steps` target in the orchestrator container
# through `aoe-harness slot`; the registry decides whether the gate takes a slot. The steps read
# the environment named in SLOT_ENV (the audit in slots/tests/makefile.rs checks the list).
SLOT_ENV := -e AOE_ASSET_PACK -e AOE_E2E_SHARD -e AOE_E2E_WORKERS -e AOE_GEODATA_CACHE -e AOE_MAP_REQUEST -e AOE_MAP_PACKAGE -e AOE_MAP_SAMPLES -e AOE_MAP_DEM_RESOLUTION -e AOE_MAP_HYDROLOGY_MODE -e AOE_POLISH_SOURCE_URL -e AOE_POLISH_SOURCE_HASH -e GIT_INDEX_FILE -e RUST_TEST_THREADS -e NEXTEST_TEST_THREADS -e AOE_SHIP_EVIDENCE -e TMPDIR -e HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE) -e HARNESS_TARGET_CACHE=$(HARNESS_TARGET_CACHE)
SLOT_RUN = docker run --rm --init $(HEAVY_ENV) $(SLOT_ENV) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) $(if $(TMPDIR),-v $(TMPDIR):$(TMPDIR)) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/debug/aoe-harness slot

# The command bootstrap is static work: one build at a time across worktrees,
# bounded to one CPU and 2 GiB including swap. Gate steps still require a slot.
SLOT_BOOTSTRAP = docker run --rm --init $(HEAVY_ENV) --memory=2g --memory-swap=2g --cpus=1 --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo build --locked --jobs=1 -p aoe-harness
.PHONY: slot-bootstrap perf-wasm-cap
slot-bootstrap: orchestrator-tools
	@flock '$(GIT_COMMON)/aoe-slot-bootstrap.lock' $(SLOT_BOOTSTRAP)

coverage: coverage-tools browser-deps orchestrator-tools build-wasm slot-bootstrap
	@$(SLOT_RUN) coverage -- make --no-print-directory coverage-steps
	@$(MAKE) --no-print-directory coverage-check

coverage-steps:
	@mkdir -p reports/coverage
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov nextest --locked --workspace --lcov --output-path reports/coverage/native.lcov
	@docker run --rm --init $(HEAVY_ENV) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e AOE_E2E_COVERAGE=1 -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-e2e-%p.profraw -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-e2e
	@docker run --rm --init $(HEAVY_ENV) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-wasm-%p.profraw -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-wasm
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --lcov --output-path reports/coverage/native.lcov
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --json --output-path reports/coverage/native.json

perf-instructions: analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-instructions -- make --no-print-directory perf-instructions-steps

perf-instructions-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench instructions -- --output-format=json > reports/perf/simulation.ndjson
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-protocol --bench instructions -- --output-format=json > reports/perf/protocol.ndjson
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-assets --bench instructions -- --output-format=json > reports/perf/assets.ndjson

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
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) wasm-opt -Oz --strip-debug web/pkg/aoe_client_bg.wasm -o reports/perf/optimized.wasm
	@docker run $(HEAVY_ENV) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) gzip -n -9 -c reports/perf/optimized.wasm > reports/perf/optimized.wasm.gz

# The local ship's cap check (preflight): compare the gzip artifact the perf-wasm-size gate just wrote with
# baselines/perf/wasm.json under the gate's own rule. It measures nothing itself (the gate requires
# perf-wasm-size), so it is light and cannot spend its budget waiting for a heavy slot.
perf-wasm-cap: slot-bootstrap
	@target/debug/aoe-harness perf-wasm-size-check

build-wasm: slot-bootstrap
	@$(SLOT_RUN) build-wasm -- make --no-print-directory build-wasm-steps

build-wasm-steps:
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm

test-e2e: build-wasm browser-deps orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-e2e -- make --no-print-directory test-e2e-steps

test-e2e-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init $(HEAVY_ENV) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -e AOE_ASSET_PACK -e AOE_E2E_SHARD -e AOE_E2E_WORKERS -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-e2e

.PHONY: test-harness test-harness-steps test-unit-steps test-wasm-steps
test-harness: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-harness -- make --no-print-directory test-harness-steps

test-harness-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness --no-fail-fast

test-unit-steps:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-unit

test-wasm-steps:
	@docker run --rm --init $(HEAVY_ENV) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-wasm
