SHELL := /bin/sh
ROOT := $(abspath .)
UID := $(shell id -u)
GID := $(shell id -g)
TOOL_IMAGE := aoeworld/rust-tools:1.93.1
BROWSER_IMAGE := aoeworld/browser-tools:1.63.0
ORCH_IMAGE := aoeworld/orchestrator:1.93.1
ANALYSIS_IMAGE := aoeworld/analysis:0.19.4
POLICY_IMAGE := aoeworld/policy:0.20.2
GIT_COMMON := $(shell git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
GIT_EXTERNAL := $(filter-out $(ROOT) $(ROOT)/%,$(GIT_COMMON))
GIT_MOUNT := $(if $(GIT_EXTERNAL),-v $(GIT_EXTERNAL):$(GIT_EXTERNAL))
ROOT_MOUNTS := $(GIT_MOUNT) -v $(ROOT):$(ROOT)
DOCKER_RUN := docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
BROWSER_RUN := docker run --rm --init --network host --ipc host --user $(UID):$(GID) -e HOME=$(ROOT)/.cache/browser-home $(ROOT_MOUNTS) -w $(ROOT)/browser $(BROWSER_IMAGE)

.PHONY: help bootstrap tools analysis-tools policy-tools browser-tools orchestrator-tools browser-deps browser-check test-e2e doctor hooks-install hooks-check structure-check architecture-check docs-check fmt fmt-check lint deny test-unit pre-commit preflight build build-wasm dev assets-inspect assets-import assets-verify perf-smoke perf-ci perf-full perf-instructions perf-wasm-size perf-baseline-propose qa-validate qa-serve release-build release-verify release-rehearse repo-policy-check

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
	@echo '  make pre-commit      Local static gate'
	@echo '  make preflight       Static gate plus native tests'
	@echo '  make build           Build workspace'
	@echo '  make build-wasm      Build and bind the Rust/WebGPU client'
	@echo '  make dev             Start synthetic server on localhost:8080'
	@echo '  make assets-inspect  Inspect ignored local-assets/trial'
	@echo '  make assets-import   Import ignored local-assets/trial'
	@echo '  make assets-verify   Verify all ignored local packs'
	@echo '  make browser-check   Typecheck, lint, and format-check browser tooling'
	@echo '  make test-e2e        Launch a disposable server and pinned Chromium'
	@echo '  make perf-smoke      Run synthetic smoke workload with real protocol clients'
	@echo '  make perf-ci         Run target workloads and required comparisons'
	@echo '  make perf-full       Add sparse-world scaling workload'
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

perf-smoke:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-smoke

perf-ci: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-ci

perf-full: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-full

perf-baseline-propose: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-baseline-propose

perf-instructions: analysis-tools
	@mkdir -p reports/perf
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench instructions -- --output-format=json > reports/perf/simulation.ndjson
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GUNGRAUN_ALLOW_ASLR=yes $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-protocol --bench instructions -- --output-format=json > reports/perf/protocol.ndjson

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

preflight: deny
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight

build:
	@$(DOCKER_RUN) cargo build --workspace --locked

build-wasm:
	@$(DOCKER_RUN) cargo build --locked --release --target wasm32-unknown-unknown -p aoe-client
	@$(DOCKER_RUN) wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/release/aoe_client.wasm

test-e2e: build-wasm browser-deps orchestrator-tools
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-e2e

dev: build-wasm
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_BIND=0.0.0.0:8080 -e AOE_SCENARIO -p 127.0.0.1:8080:8080 $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked --release -p aoe-server

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
