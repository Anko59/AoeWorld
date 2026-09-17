SHELL := /bin/sh
ROOT := $(abspath .)
UID := $(shell id -u)
GID := $(shell id -g)
TOOL_IMAGE := aoeworld/rust-tools:1.93.1
BROWSER_IMAGE := aoeworld/browser-tools:1.63.0
ORCH_IMAGE := aoeworld/orchestrator:1.93.1
ANALYSIS_IMAGE := aoeworld/analysis:0.19.4
POLICY_IMAGE := aoeworld/policy:0.20.2
COVERAGE_IMAGE := aoeworld/coverage:0.9.1
FUZZ_IMAGE := aoeworld/fuzz:nightly-2026-09-01-0.13.2
MUTATION_IMAGE := aoeworld/mutation:27.1.0
GIT_COMMON := $(shell git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
GIT_EXTERNAL := $(filter-out $(ROOT) $(ROOT)/%,$(GIT_COMMON))
GIT_MOUNT := $(if $(GIT_EXTERNAL),-v $(GIT_EXTERNAL):$(GIT_EXTERNAL))
GITHUB_OUTPUT_MOUNT := $(if $(GITHUB_OUTPUT),-v $(GITHUB_OUTPUT):$(GITHUB_OUTPUT))
ROOT_MOUNTS := $(GIT_MOUNT) -v $(ROOT):$(ROOT)
DOCKER_RUN := docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
BROWSER_RUN := docker run --rm --init --network host --ipc host --user $(UID):$(GID) -e HOME=$(ROOT)/.cache/browser-home $(ROOT_MOUNTS) -w $(ROOT)/browser $(BROWSER_IMAGE)
DEV_ORCH_RUN := docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_SCENARIO $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE)

.PHONY: help bootstrap tools analysis-tools policy-tools coverage-tools fuzz-tools mutation-tools browser-tools orchestrator-tools browser-deps browser-check test-wasm test-e2e fuzz-smoke fuzz-nightly mutation-nightly doctor hooks-install hooks-check structure-check architecture-check docs-check fmt fmt-check lint deny test-unit coverage coverage-check ci-select ci-check pre-commit preflight build build-wasm dev down status logs assets-inspect assets-import assets-verify perf-smoke perf-ci perf-full perf-pressure perf-stress perf-soak-10 perf-soak-30 perf-instructions perf-wasm-size perf-baseline-propose qa-validate qa-serve release-build release-verify release-rehearse repo-policy-check

help:
	@echo 'AoeWorld Harness Lab'
	@echo '  make bootstrap       Build the pinned Rust tools image and install hooks'
	@echo '  make doctor          Check toolchain availability'
	@echo '  make fmt-check       Check Rust formatting without changing files'
	@echo '  make fmt             Format Rust files'
	@echo '  make structure-check Check tracked file and directory limits'
	@echo '  make architecture-check Check crate boundaries and source rules'
	@echo '  make docs-check      Check gate table and local documentation links'
	@echo '  make lint            Run strict Clippy'
	@echo '  make deny            Audit Cargo advisories, licenses, bans, and sources'
	@echo '  make test-unit       Run native tests'
	@echo '  make coverage        Check native production-line coverage floors'
	@echo '  make pre-commit      Local static gate'
	@echo '  make preflight       Static gate plus native tests'
	@echo '  make ci-select       Emit revision-bound selected CI jobs'
	@echo '  make ci-check        Validate selection manifest and job outcomes'
	@echo '  make build           Build workspace'
	@echo '  make build-wasm      Build and bind the Rust/WebGPU client'
	@echo '  make dev             Start checkout-scoped lab on localhost:8080'
	@echo '  make down            Stop this checkout’s lab'
	@echo '  make status          Show this checkout’s lab status'
	@echo '  make logs            Show recent lab logs'
	@echo '  make assets-inspect  Inspect ignored local-assets/trial'
	@echo '  make assets-import   Import ignored local-assets/trial'
	@echo '  make assets-verify   Verify all ignored local packs'
	@echo '  make browser-check   Typecheck, lint, and format-check browser tooling'
	@echo '  make test-e2e        Launch a disposable server and pinned Chromium'
	@echo '  make test-wasm       Execute wasm-bindgen tests in pinned Chromium'
	@echo '  make fuzz-smoke      Run bounded parser fuzzing with pinned nightly Rust'
	@echo '  make fuzz-nightly    Run longer parser fuzzing campaigns'
	@echo '  make mutation-nightly  Check critical gate and comparator mutations'
	@echo '  make perf-smoke      Run synthetic smoke workload with real protocol clients'
	@echo '  make perf-ci         Run target workloads and required comparisons'
	@echo '  make perf-full       Add population and sparse-world scaling workloads'
	@echo '  make perf-pressure   Schedule independent network offers and camera changes'
	@echo '  make perf-stress     Characterize explicit beyond-target overload'
	@echo '  make perf-soak-10    Repeat target network lifecycle for 10 minutes'
	@echo '  make perf-soak-30    Repeat target network lifecycle for 30 minutes'
	@echo '  make perf-instructions Run pinned Gungraun/Callgrind kernels'
	@echo '  make perf-baseline-propose Write a reviewable baseline proposal'
	@echo '  make qa-validate     Validate reports/qa/session.json'
	@echo '  make qa-serve        Serve restricted MCP browser tools on stdio'
	@echo '  make release-build  Build local runtime images from clean dev with evidence'
	@echo '  AOE_RELEASE_MANIFEST=... make release-verify  Verify a local release'
	@echo '  AOE_RELEASE_CANDIDATE=... AOE_RELEASE_PREVIOUS=... make release-rehearse'
	@echo '  make repo-policy-check  Audit GitHub branch and merge settings'

.cache/cargo:
	@mkdir -p .cache/cargo

tools: .cache/cargo
	@docker build -f docker/rust-tools.Dockerfile -t $(TOOL_IMAGE) .

browser-tools:
	@docker build -f docker/browser-tools.Dockerfile -t $(BROWSER_IMAGE) .

analysis-tools: tools
	@docker build -f docker/analysis.Dockerfile -t $(ANALYSIS_IMAGE) .

policy-tools: tools
	@docker build -f docker/policy.Dockerfile -t $(POLICY_IMAGE) .

coverage-tools: tools
	@docker build -f docker/coverage.Dockerfile -t $(COVERAGE_IMAGE) .

fuzz-tools: tools
	@docker build -f docker/fuzz.Dockerfile -t $(FUZZ_IMAGE) .

mutation-tools:
	@docker build -f docker/mutation.Dockerfile -t $(MUTATION_IMAGE) .

orchestrator-tools: tools
	@docker build -f docker/orchestrator.Dockerfile -t $(ORCH_IMAGE) .

browser-deps: browser-tools
	@mkdir -p .cache/browser-home
	@$(BROWSER_RUN) npm ci --ignore-scripts

browser-check: browser-deps
	@$(BROWSER_RUN) npm run typecheck
	@$(BROWSER_RUN) npm run lint
	@$(BROWSER_RUN) npm run format-check

bootstrap: orchestrator-tools browser-deps
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-install
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-check

doctor:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- doctor

hooks-install:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-install

hooks-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-check

structure-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- structure-check

architecture-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- architecture-check

docs-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- docs-check

fmt-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fmt-check

fmt:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fmt

lint:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- lint

deny: policy-tools
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(POLICY_IMAGE) cargo deny --locked check

test-unit:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-unit

test-wasm: browser-tools orchestrator-tools
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-wasm

fuzz-smoke: fuzz-tools
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-smoke

fuzz-nightly: fuzz-tools
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT)/fuzz $(FUZZ_IMAGE) cargo run --locked --manifest-path $(ROOT)/crates/harness/Cargo.toml -- fuzz-nightly

mutation-nightly: mutation-tools
	@mkdir -p reports/mutation
	@docker run --rm --init --user $(UID):$(GID) --tmpfs /tmp:rw,exec,size=4g -e CARGO_HOME=$(ROOT)/.cache/cargo -e CARGO_TARGET_DIR=/tmp/aoeworld-mutation-target $(ROOT_MOUNTS) -w $(ROOT) $(MUTATION_IMAGE) cargo run --locked -p aoe-harness -- mutation-nightly

coverage: coverage-tools browser-deps orchestrator-tools build-wasm
	@mkdir -p reports/coverage
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov nextest --locked --workspace --lcov --output-path reports/coverage/native.lcov
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-e2e-%p.profraw -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-e2e
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e LLVM_PROFILE_FILE=$(ROOT)/target/llvm-cov-target/coverage-wasm-%p.profraw -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) target/llvm-cov-target/debug/aoe-harness test-wasm
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --lcov --output-path reports/coverage/native.lcov
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --json --output-path reports/coverage/native.json
	@$(MAKE) --no-print-directory coverage-check

coverage-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- coverage-check

ci-select:
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_BASE_SHA -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-select

ci-check:
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_BASE_SHA -e AOE_SELECTION_JSON -e AOE_JOB_RESULTS_JSON $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-check

perf-smoke:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-smoke

perf-ci: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-ci

perf-full: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-full

perf-pressure:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-pressure

perf-stress:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-stress

perf-soak-10:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-soak-10

perf-soak-30:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-soak-30

perf-baseline-propose: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-baseline-propose

perf-instructions: analysis-tools
	@mkdir -p reports/perf
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench instructions -- --output-format=json > reports/perf/simulation.ndjson
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-protocol --bench instructions -- --output-format=json > reports/perf/protocol.ndjson
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-assets --bench instructions -- --output-format=json > reports/perf/assets.ndjson

perf-wasm-size: build-wasm analysis-tools
	@mkdir -p reports/perf
	@docker run --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) wasm-opt -Oz --strip-debug web/pkg/aoe_client_bg.wasm -o reports/perf/optimized.wasm
	@docker run --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) gzip -n -9 -c reports/perf/optimized.wasm > reports/perf/optimized.wasm.gz

qa-validate:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- qa-validate

qa-serve:
	@$(MAKE) --no-print-directory orchestrator-tools browser-deps 1>&2
	@docker run --rm --init -i --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_QA_BASE_URL $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- qa-serve

pre-commit:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- pre-commit

preflight: deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight

build:
	@$(DOCKER_RUN) cargo build --workspace --locked

build-wasm:
	@$(DOCKER_RUN) cargo build --locked --release --target wasm32-unknown-unknown -p aoe-client
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/release/aoe_client.wasm

test-e2e: build-wasm browser-deps orchestrator-tools
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-e2e

dev: build-wasm orchestrator-tools
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev start

down: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev down

status: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev status

logs: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev logs

assets-inspect:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets inspect

assets-import:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets import

assets-verify:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets verify

release-build: orchestrator-tools
	@docker run --rm --init --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-build

release-verify: orchestrator-tools
	@docker run --rm --init --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_RELEASE_MANIFEST $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-verify

release-rehearse: orchestrator-tools
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_RELEASE_CANDIDATE -e AOE_RELEASE_PREVIOUS $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-rehearse

repo-policy-check:
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GITHUB_TOKEN $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- repo-policy-check
