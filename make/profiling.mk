# Same army builder as perf-groups; instrument movement at tick 201, then whole ticks per stage
# (instructions and cache-simulation LL misses, reports/perf/group-stages.json).
.PHONY: perf-groups-instructions perf-groups-instructions-steps
perf-groups-instructions: resource-bootstrap
	@$(RESOURCE_LONG) perf-groups-instructions -- $(MAKE) --no-print-directory perf-groups-instructions-admitted

.PHONY: perf-groups-instructions-admitted
perf-groups-instructions-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-groups-instructions-steps

perf-groups-instructions-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes -e CARGO_PROFILE_BENCH_DEBUG=1 $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-simulation --bench group_instructions -- --output-format=json --save-summary=json > reports/perf/group-instructions.ndjson
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-stage-counters

GROUP_CALLGRIND ?= target/gungraun/aoe-simulation/group_instructions/simulation/movement/callgrind.movement.out
GROUP_CALLGRIND_REPORT ?= reports/perf/group-callgrind-inclusive.txt
GROUP_CALLGRIND_INCLUSIVE ?= yes
.PHONY: perf-groups-annotate
perf-groups-annotate:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) callgrind_annotate --show=Ir --inclusive=$(GROUP_CALLGRIND_INCLUSIVE) --threshold=100 --auto=no $(GROUP_CALLGRIND) > $(GROUP_CALLGRIND_REPORT)

# Keep the matched timing and instruction measurements in one admitted slot.
.PHONY: perf-groups-profile perf-groups-profile-steps
perf-groups-profile: resource-bootstrap
	@$(RESOURCE_LONG) perf-groups-profile -- $(MAKE) --no-print-directory perf-groups-profile-admitted

.PHONY: perf-groups-profile-admitted
perf-groups-profile-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-groups-profile-steps

perf-groups-profile-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@make --no-print-directory perf-groups-steps
	@make --no-print-directory perf-groups-instructions-steps

# Private economy reports at the 1,000-player target; counts exclude fixture setup.
.PHONY: perf-census-instructions perf-census-instructions-admitted perf-census-instructions-steps
perf-census-instructions: resource-bootstrap
	@$(RESOURCE_LONG) perf-census-instructions -- $(MAKE) --no-print-directory perf-census-instructions-admitted

perf-census-instructions-admitted: job-bootstrap
	@$(MAKE) --no-print-directory perf-census-instructions-steps

perf-census-instructions-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@mkdir -p reports/perf
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes -e CARGO_PROFILE_BENCH_DEBUG=1 $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo bench --locked -p aoe-server --features bench-fixtures --bench census_instructions -- --output-format=json --save-summary=json > reports/perf/census-instructions.ndjson

# Closed-loop in-process late-game match with Rust bots (movement rework phase 0, rung 0):
# ticks/s and phases per scale and worker count, hashes compared across workers.
# MATCH_BENCH_ARGS passes harness flags, e.g. "--seats 1000 --ticks 2000" or "--scale 1,5,10 --callgrind".
.PHONY: match-bench match-bench-admitted match-bench-steps
match-bench: resource-bootstrap
	@$(RESOURCE_LONG) match-bench -- $(MAKE) --no-print-directory match-bench-admitted

match-bench-admitted: job-bootstrap
	@$(MAKE) --no-print-directory match-bench-steps

match-bench-steps:
	@$(MAKE) --no-print-directory analysis-tools
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) $(RESOURCE_OBSERVER) cargo run --release --locked -p aoe-harness -- match-bench $(MATCH_BENCH_ARGS)

# Movement apply census: the ignored 100-seat profiling test, under the long-job lock.
.PHONY: movement-apply-census movement-apply-census-admitted movement-apply-census-steps
movement-apply-census: resource-bootstrap
	@$(RESOURCE_LONG) movement-apply-census -- $(MAKE) --no-print-directory movement-apply-census-admitted

movement-apply-census-admitted: job-bootstrap
	@$(MAKE) --no-print-directory movement-apply-census-steps

movement-apply-census-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness late_game_apply_census -- --ignored

# Replays local saved-game commands; all work runs in the pinned tool container.
.PHONY: client-frame-bench client-frame-bench-build client-frame-bench-steps
client-frame-bench: resource-bootstrap
	@$(RESOURCE_LONG) client-frame-bench -- $(MAKE) --no-print-directory client-frame-bench-steps

# Private WASM output does not replace the ordinary gameplay artifact.
client-frame-bench-build: job-bootstrap
	@$(MAKE) --no-print-directory browser-deps
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets browser-shaders
	@$(DOCKER_RUN) cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features frame-bench
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir .cache/client-frame-bench/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server

# Build only the instrumented historical client, with a private Cargo target.
.PHONY: client-frame-bench-client-build
client-frame-bench-client-build:
	@$(DOCKER_RUN) cargo build --manifest-path '$(CLIENT_FRAME_SOURCE)/Cargo.toml' --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features frame-bench --target-dir '$(CLIENT_FRAME_SOURCE)/target'
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir '$(CLIENT_FRAME_SOURCE)/.cache/client-frame-bench/pkg' --out-name aoe_client '$(CLIENT_FRAME_SOURCE)/target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm'

client-frame-bench-steps: client-frame-bench-build
	@'$(JOB_BINARY)' client-frame-bench $(CLIENT_FRAME_BENCH_ARGS)

.PHONY: client-frame-bench-browser-test
client-frame-bench-browser-test: browser-deps
	@$(BROWSER_RUN) node --test bench/counters_tests.mjs

.PHONY: client-frame-bench-format
client-frame-bench-format: browser-deps
	@$(BROWSER_RUN) npx prettier --write bench

.PHONY: client-frame-bench-check
client-frame-bench-check: job-bootstrap
	@$(DOCKER_RUN) cargo check --locked --target wasm32-unknown-unknown -p aoe-client --features frame-bench

.PHONY: client-frame-bench-compile-load
CLIENT_FRAME_LOAD_TARGET ?= $(ROOT)/.cache/client-frame-bench/load-target
client-frame-bench-compile-load:
	@$(DOCKER_RUN) env $(call MOUNT_QUOTE,CARGO_TARGET_DIR=$(CLIENT_FRAME_LOAD_TARGET)) $(call MOUNT_QUOTE,RUSTFLAGS=$(CLIENT_FRAME_LOAD_RUSTFLAGS)) CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --locked --release -p aoe-server -p aoe-harness

.PHONY: save-replay save-replay-admitted save-replay-steps
save-replay: resource-bootstrap
	@$(RESOURCE_LONG) save-replay -- $(MAKE) --no-print-directory save-replay-admitted
save-replay-admitted: job-bootstrap
	@$(MAKE) --no-print-directory save-replay-steps
save-replay-steps:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --cap-add PERFMON --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e $(call MOUNT_QUOTE,CARGO_TARGET_DIR=$(ROOT)/.cache/save-replay/target) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) $(RESOURCE_OBSERVER) cargo run --release --locked -p aoe-harness -- save-replay $(AOE_SAVE_REPLAY_ARGS)

# Explicit PMU access check; ordinary unit tests do not require host counters.
.PHONY: save-replay-counter-check
save-replay-counter-check:
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --cap-add PERFMON --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) $(RESOURCE_OBSERVER) cargo test --locked -p aoe-server hardware_counter_is_user_only_thread_local_and_fully_scheduled -- --ignored
