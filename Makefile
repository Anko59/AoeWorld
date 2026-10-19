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
# CI sets e.g. type=gha,version=2 to reuse BuildKit layers; empty keeps local builds unchanged.
DOCKER_BUILD_CACHE ?=
COMMA := ,
# Quote complete CSV fields for Docker binds; colons stay ordinary path data.
MOUNT_QUOTE = '$(subst ','"'"',$(1))'
MOUNT_FIELD = "$(subst ","",$(1))"
BIND_MOUNT = --mount $(call MOUNT_QUOTE,type=bind$(COMMA)$(call MOUNT_FIELD,source=$(1))$(COMMA)$(call MOUNT_FIELD,target=$(2))$(if $(3),$(COMMA)readonly))
IMAGE_BUILD = docker build $(if $(DOCKER_BUILD_CACHE),--load --cache-from $(DOCKER_BUILD_CACHE)$(COMMA)scope=$(1) --cache-to $(DOCKER_BUILD_CACHE)$(COMMA)scope=$(1)$(COMMA)mode=max$(COMMA)ignore-error=true) -f docker/$(1).Dockerfile -t $(2) .
GIT_COMMON := $(shell git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
GIT_EXTERNAL := $(filter-out $(ROOT) $(ROOT)/%,$(GIT_COMMON))
GIT_MOUNT := $(if $(GIT_EXTERNAL),$(call BIND_MOUNT,$(GIT_EXTERNAL),$(GIT_EXTERNAL)))
GITHUB_OUTPUT_MOUNT := $(if $(GITHUB_OUTPUT),-v $(GITHUB_OUTPUT):$(GITHUB_OUTPUT))
HARNESS_CARGO_CACHE ?= $(ROOT)/.cache/cargo
HARNESS_TARGET_CACHE ?= $(ROOT)/target
CACHE_MOUNTS := $(if $(filter-out $(ROOT)/.cache/cargo,$(HARNESS_CARGO_CACHE)),$(call BIND_MOUNT,$(HARNESS_CARGO_CACHE),$(ROOT)/.cache/cargo)) $(if $(filter-out $(ROOT)/target,$(HARNESS_TARGET_CACHE)),$(call BIND_MOUNT,$(HARNESS_TARGET_CACHE),$(ROOT)/target))
HEAVY_ENV := -e GITHUB_ACTIONS -e AOE_TEST_SELECT -e AOE_TEST_ALL -e AOE_HEAVY_SLOT -e AOE_SLOT_CPUS -e AOE_HEAVY_DIR -e AOE_HEAVY_SLOTS -e AOE_HEAVY_WAIT_MINUTES
# The CPUs each slot gives its containers: gates/heavy.json cores shared among its slots. The
# harness that holds the slot computes the count and exports it as AOE_SLOT_CPUS beside
# AOE_HEAVY_SLOT; make only expands that variable, so there is no host binary to find. A container
# started under a slot takes `--cpus` and cargo the same count as its job limit. With no slot the
# variable is unset and SLOT_CAP is empty (never an empty flag), so static gates keep the machine.
SLOT_CAP = $(if $(and $(AOE_HEAVY_SLOT),$(AOE_SLOT_CPUS)),--cpus=$(AOE_SLOT_CPUS) -e CARGO_BUILD_JOBS=$(AOE_SLOT_CPUS))
# A snapshot's slot directory is the real repository's Git directory: mount it unless a mount covers it.
HEAVY_DIR_COVERED := $(ROOT) $(ROOT)/% $(if $(GIT_EXTERNAL),$(GIT_EXTERNAL) $(GIT_EXTERNAL)/%)
HEAVY_DIR_EXTERNAL := $(if $(AOE_HEAVY_DIR),$(filter-out $(HEAVY_DIR_COVERED),$(AOE_HEAVY_DIR)))
HEAVY_DIR_MOUNT := $(if $(HEAVY_DIR_EXTERNAL),-v $(HEAVY_DIR_EXTERNAL):$(HEAVY_DIR_EXTERNAL))
TMP_MOUNT := $(if $(TMPDIR),-v $(TMPDIR):/aoe-harness-tmp -e TMPDIR=/aoe-harness-tmp)
ROOT_MOUNTS := $(GIT_MOUNT) $(HEAVY_DIR_MOUNT) $(call BIND_MOUNT,$(ROOT),$(ROOT)) $(CACHE_MOUNTS)
DOCKER_RUN := docker run --rm --init --label aoe.harness-boot-gap=$(AOE_HARNESS_BOOT_GAP) $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GIT_INDEX_FILE -e RUST_TEST_THREADS -e NEXTEST_TEST_THREADS $(TMP_MOUNT) -e AOE_SHIP_EVIDENCE $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
GEODATA_PATHS := $(AOE_GEODATA_CACHE) $(AOE_MAP_REQUEST) $(AOE_MAP_PACKAGE)
GEODATA_EXTERNAL_DIRS := $(filter-out $(ROOT) $(ROOT)/%,$(sort $(foreach path,$(filter /%,$(GEODATA_PATHS)),$(patsubst %/,%,$(dir $(path))))))
GEODATA_MOUNTS := $(foreach directory,$(GEODATA_EXTERNAL_DIRS),-v $(directory):$(directory))
GEODATA_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_GEODATA_CACHE -e AOE_MAP_REQUEST -e AOE_MAP_PACKAGE -e AOE_MAP_SAMPLES -e AOE_MAP_DEM_RESOLUTION -e AOE_MAP_HYDROLOGY_MODE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
SOURCE_QUAL_PATHS := $(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY)
SOURCE_QUAL_EXTERNAL_DIRS := $(filter-out $(ROOT) $(ROOT)/%,$(sort $(foreach path,$(filter /%,$(SOURCE_QUAL_PATHS)),$(patsubst %/,%,$(dir $(path))))))
SOURCE_QUAL_MOUNTS := $(foreach directory,$(SOURCE_QUAL_EXTERNAL_DIRS),-v $(directory):$(directory))
SOURCE_QUAL_CARGO_HOME ?= $(ROOT)/.cache/cargo
SOURCE_QUAL_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(SOURCE_QUAL_CARGO_HOME) $(ROOT_MOUNTS) $(SOURCE_QUAL_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
SOURCE_QUAL_GEOGRAPHIC_ARGS := $(if $(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY),--geographic-package-directory '$(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_GEOGRAPHIC_CONTENT_HASH),--geographic-content-hash '$(AOE_SOURCE_QUAL_GEOGRAPHIC_CONTENT_HASH)')
SOURCE_QUAL_SCALE_ARGS := $(if $(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY),--scale-512-package-directory '$(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_512_CONTENT_HASH),--scale-512-content-hash '$(AOE_SOURCE_QUAL_512_CONTENT_HASH)') $(if $(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY),--scale-16384-package-directory '$(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_16384_CONTENT_HASH),--scale-16384-content-hash '$(AOE_SOURCE_QUAL_16384_CONTENT_HASH)') $(if $(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY),--scale-262144-package-directory '$(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_262144_CONTENT_HASH),--scale-262144-content-hash '$(AOE_SOURCE_QUAL_262144_CONTENT_HASH)')
BROWSER_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --ipc host --user $(UID):$(GID) -e HOME=$(ROOT)/.cache/browser-home $(ROOT_MOUNTS) -w $(ROOT)/browser $(BROWSER_IMAGE)
DEV_ORCH_BASE := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_SCENARIO -e AOE_ASSET_PACK -e AOE_OPEN_MAP_PACKAGE -e DEV_PORT $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT)
DEV_ORCH_RUN := $(DEV_ORCH_BASE) $(ORCH_IMAGE)
HARNESS_EVIDENCE_DIR ?= /tmp/aoeworld-harness-evidence
HARNESS_CADENCE ?= edit
HARNESS_SCOPE ?= working
override HARNESS_LOCAL_RUN = docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g $(HARNESS_ACTIVE_DOCKER_SOCKET_REAL)) -e CARGO_HOME=$(HARNESS_ROOT_REAL)/.cache/cargo -e AOE_SCENARIO -e AOE_ASSET_PACK $(HEAVY_DIR_MOUNT) -v $(HARNESS_ROOT_REAL):$(HARNESS_ROOT_REAL) $(if $(HARNESS_GIT_EXTERNAL_REAL),-v $(HARNESS_GIT_EXTERNAL_REAL):$(HARNESS_GIT_EXTERNAL_REAL)) $(if $(HARNESS_CARGO_CACHE_MOUNT_SOURCE),-v $(HARNESS_CARGO_CACHE_REAL):$(HARNESS_ROOT_REAL)/.cache/cargo) $(if $(HARNESS_TARGET_CACHE_MOUNT_SOURCE),-v $(HARNESS_TARGET_CACHE_REAL):$(HARNESS_ROOT_REAL)/target) -v $(HARNESS_ACTIVE_DOCKER_SOCKET_REAL):/var/run/docker.sock -w $(HARNESS_ROOT_REAL) -v $(HARNESS_EVIDENCE_REAL):$(HARNESS_EVIDENCE_REAL) $(ORCH_IMAGE)
override HARNESS_ROOT_REAL := $(realpath $(ROOT))
override HARNESS_GIT_EXTERNAL_REAL := $(realpath $(GIT_EXTERNAL))
override HARNESS_CARGO_CACHE_MOUNT_SOURCE := $(filter-out $(ROOT)/.cache/cargo,$(HARNESS_CARGO_CACHE))
override HARNESS_TARGET_CACHE_MOUNT_SOURCE := $(filter-out $(ROOT)/target,$(HARNESS_TARGET_CACHE))
override HARNESS_CARGO_CACHE_REAL := $(realpath $(HARNESS_CARGO_CACHE))
override HARNESS_TARGET_CACHE_REAL := $(realpath $(HARNESS_TARGET_CACHE))
override HARNESS_CACHE_MOUNT_SOURCES_REAL := $(if $(HARNESS_CARGO_CACHE_MOUNT_SOURCE),$(HARNESS_CARGO_CACHE_REAL)) $(if $(HARNESS_TARGET_CACHE_MOUNT_SOURCE),$(HARNESS_TARGET_CACHE_REAL))
override HARNESS_RUNTIME_DIRS_REAL := $(realpath /run) $(realpath /proc) $(realpath /sys) $(realpath /dev)
override HARNESS_DOCKER_CONTEXT_ENDPOINT = $(shell docker context inspect --format '{{.Endpoints.docker.Host}}' 2>/dev/null)
override HARNESS_ACTIVE_DOCKER_ENDPOINT = $(if $(strip $(DOCKER_CONTEXT)),$(HARNESS_DOCKER_CONTEXT_ENDPOINT),$(if $(strip $(DOCKER_HOST)),$(DOCKER_HOST),$(HARNESS_DOCKER_CONTEXT_ENDPOINT)))
override HARNESS_ACTIVE_DOCKER_SOCKET_REAL = $(if $(filter unix://%,$(HARNESS_ACTIVE_DOCKER_ENDPOINT)),$(realpath $(patsubst unix://%,%,$(HARNESS_ACTIVE_DOCKER_ENDPOINT))))
override HARNESS_DOCKER_SOCKET_PATHS_REAL = $(realpath /var/run/docker.sock) $(if $(findstring unix://,$(DOCKER_HOST)),$(realpath $(patsubst unix://%,%,$(DOCKER_HOST)))) $(if $(findstring unix://,$(HARNESS_DOCKER_CONTEXT_ENDPOINT)),$(realpath $(patsubst unix://%,%,$(HARNESS_DOCKER_CONTEXT_ENDPOINT))))
override HARNESS_EVIDENCE_REAL := $(realpath $(HARNESS_EVIDENCE_DIR))

# Bind mounts erase host aliases; reject evidence sharing a host source with any RW mount.
define HARNESS_VALIDATE_EVIDENCE_PATH
$(if $(strip $(HARNESS_EVIDENCE_REAL)),,$(error HARNESS_EVIDENCE_DIR must resolve to an existing directory))$(if $(strip $(DOCKER_HOST)),$(if $(filter unix://%,$(DOCKER_HOST)),,$(error harness evidence mounts require a local Unix Docker endpoint)),)$(if $(strip $(HARNESS_DOCKER_CONTEXT_ENDPOINT)),$(if $(filter unix://%,$(HARNESS_DOCKER_CONTEXT_ENDPOINT)),,$(error harness evidence mounts require a local Unix Docker context)),)$(if $(filter /,$(HARNESS_EVIDENCE_REAL)),$(error HARNESS_EVIDENCE_DIR must not be the host filesystem root))$(if $(filter $(HARNESS_ROOT_REAL) $(HARNESS_ROOT_REAL)/%,$(HARNESS_EVIDENCE_REAL))$(filter $(HARNESS_EVIDENCE_REAL) $(HARNESS_EVIDENCE_REAL)/%,$(HARNESS_ROOT_REAL)),$(error HARNESS_EVIDENCE_DIR must be disjoint from the checkout))$(if $(HARNESS_GIT_EXTERNAL_REAL),$(if $(filter $(HARNESS_GIT_EXTERNAL_REAL) $(HARNESS_GIT_EXTERNAL_REAL)/%,$(HARNESS_EVIDENCE_REAL))$(filter $(HARNESS_EVIDENCE_REAL) $(HARNESS_EVIDENCE_REAL)/%,$(HARNESS_GIT_EXTERNAL_REAL)),$(error HARNESS_EVIDENCE_DIR must be disjoint from external Git metadata)),)$(if $(HARNESS_CARGO_CACHE_MOUNT_SOURCE),$(if $(strip $(HARNESS_CARGO_CACHE_REAL)),,$(error custom HARNESS_CARGO_CACHE mounts must resolve to an existing path)),)$(if $(HARNESS_TARGET_CACHE_MOUNT_SOURCE),$(if $(strip $(HARNESS_TARGET_CACHE_REAL)),,$(error custom HARNESS_TARGET_CACHE mounts must resolve to an existing path)),)$(foreach cache,$(HARNESS_CACHE_MOUNT_SOURCES_REAL),$(if $(strip $(cache)),$(if $(filter $(cache) $(cache)/%,$(HARNESS_EVIDENCE_REAL))$(filter $(HARNESS_EVIDENCE_REAL) $(HARNESS_EVIDENCE_REAL)/%,$(cache)),$(error HARNESS_EVIDENCE_DIR must be disjoint from writable cache mounts)),))$(foreach runtime,$(HARNESS_RUNTIME_DIRS_REAL),$(if $(strip $(runtime)),$(if $(filter $(runtime) $(runtime)/%,$(HARNESS_EVIDENCE_REAL)),$(error HARNESS_EVIDENCE_DIR must not expose host runtime paths)),))$(foreach socket,$(HARNESS_DOCKER_SOCKET_PATHS_REAL),$(if $(strip $(socket)),$(if $(filter $(HARNESS_EVIDENCE_REAL) $(HARNESS_EVIDENCE_REAL)/%,$(socket)),$(error HARNESS_EVIDENCE_DIR must not contain a Docker socket)),))
endef

.PHONY: help bootstrap tools analysis-tools policy-tools coverage-tools fuzz-tools mutation-tools browser-tools orchestrator-tools browser-deps browser-check test-wasm test-e2e test-creator-source test-geographic-matrix fuzz-smoke fuzz-nightly mutation-nightly doctor hooks-install hooks-check test-placement structure-check architecture-check docs-check fmt fmt-check lint deny test-unit test-source-qualification test-exhaustive geodata-bootstrap geodata-verify map-estimate map-generate map-generate-detailed map-verify map-test map-perf map-source-qualify coverage coverage-check ci-select ci-check ci-dispatch-status pre-commit pre-push preflight preflight-checks preflight-unit build build-wasm dev down status logs assets-inspect assets-import assets-import-de assets-verify assets-dat-table perf-smoke perf-ci perf-full perf-pressure perf-stress perf-soak-10 perf-soak-30 perf-instructions perf-timing perf-wasm-size perf-baseline-propose perf-comment perf-hardware-check qa-validate qa-serve release-build release-publish release-source-check release-main-source-check release-verify-published release-rehearse release-smoke-published release-verify repo-policy-check map-source-scale-qualify release-rehearse-published test-geographic-visuals test-geographic-visuals-unit recover

help:
	@echo 'AoeWorld'
	@echo '  make bootstrap       Build the pinned Rust tools image and install hooks'
	@echo '  make timeline        Report PR delivery stages (last 7 days)'
	@echo '  make doctor          Check toolchain availability'
	@echo '  make recover         Stop harness containers a host suspend left running'
	@echo '  make fmt-check       Check Rust formatting without changing files'
	@echo '  make fmt             Format Rust files'
	@echo '  make test-placement  Reject additions to inline Rust tests'
	@echo '  make structure-check Check tracked file and directory limits'
	@echo '  make architecture-check Check crate boundaries and source rules'
	@echo '  make docs-check      Check gate table and local documentation links'
	@echo '  make lint            Run strict Clippy'
	@echo '  make deny            Audit Cargo advisories, licenses, bans, and sources'
	@echo '  make test-unit       Run native tests (except the post-merge tiers)'
	@echo '  make test-source-qualification  Run the slow source-qualification tests test-unit excludes'
	@echo '  make test-exhaustive  Run the 16-seed sweeps and full-axis geodata round-trips test-unit excludes'
	@echo '  make load-status     Show the heavy slot holders and queue'
	@echo '  make ledger          Which model did what with measured results (docs/models.md)'
	@echo '  make usage           What is left of each provider, and the last 7 days of the ledger'
	@echo '  make geodata-bootstrap Acquire the configured overview source stack'
	@echo '  make geodata-verify  Verify cached overview inputs without network access'
	@echo '  AOE_MAP_REQUEST=... make map-estimate  Estimate a map request'
	@echo '  AOE_MAP_REQUEST=... AOE_MAP_PACKAGE=... make map-generate  Create a self-contained package'
	@echo '  AOE_MAP_REQUEST=... AOE_MAP_PACKAGE=... make map-generate-detailed  Create a bounded public DEM package'
	@echo '  AOE_MAP_PACKAGE=... make map-verify  Verify a generated package'
	@echo '  AOE_SOURCE_QUAL_PACKAGE_DIRECTORY=... AOE_SOURCE_QUAL_CONTENT_HASH=... make map-source-qualify  Qualify the 50,000-tile, 1:1 source-backed 100km case'
	@echo '  Optional scale references: AOE_SOURCE_QUAL_512_*, AOE_SOURCE_QUAL_16384_*, AOE_SOURCE_QUAL_262144_*'
	@echo '  make map-source-scale-qualify  Sample verified source packages at the configured scale axes without running movement'
	@echo '  make map-test        Run deterministic map-model tests'
	@echo '  AOE_MAP_REQUEST=... make map-perf  Sample synthetic map-generation work'
	@echo '  make duel DUEL_TASK=<file>  Run tester red then implementer green (docs/shipping.md)'
	@echo '  make red             Prove the changed tests of the PR red on the merge base and green at HEAD'
	@echo '  make coverage        Check native production-line coverage floors'
	@echo '  .agents/hooks/harness.sh build  Build the agent hook judge (docs/agent-runtimes.md)'
	@echo '  make pre-commit      Local static gate'
	@echo '  make preflight       Static gate plus native tests'
	@echo '  make preflight-checks / preflight-unit  The two halves of preflight (CI runs them in parallel)'
	@echo '  make ci-select       Emit revision-bound selected CI jobs'
	@echo '  make ci-dispatch-status Report a dispatch aggregate as a commit status'
	@echo '  make ci-check        Validate selection manifest and job outcomes'
	@echo '  make build           Build workspace'
	@echo '  make build-wasm      Build and bind the Rust/WebGPU client'
	@echo '  make dev             Start checkout-scoped lab on localhost:8080 (DEV_PORT)'
	@echo '  make down            Stop this checkout’s lab'
	@echo '  make status          Show this checkout’s lab status'
	@echo '  make logs            Show recent lab logs'
	@echo '  make assets-inspect  Inspect ignored local-assets/trial'
	@echo '  make assets-import   Import ignored local-assets/trial'
	@echo '  make assets-import-de  Import Dark Age buildings from a Definitive Edition install (DE_DIR)'
	@echo '  make test-assets-de  Test real DE house decoding and player masks (DE_DIR, read-only)'
	@echo '  make assets-verify   Verify all ignored local packs'
	@echo '  make assets-dat-table  Decode empires2.dat into local-assets/dat-table (unit to sprite ids)'
	@echo '  make browser-check   Typecheck, lint, and format-check browser tooling'
	@echo '  make dev-server      Build and keep one dev server tracking origin/dev, with preset maps (-status, -stop; docs/dev-server.md)'
	@echo '  make compare         Screenshot bookmarked cameras on the build before and after the change (docs/showcase.md)'
	@echo '  make test-e2e        Launch a disposable server and pinned Chromium'
	@echo '  AOE_POLISH_SOURCE_URL=... AOE_POLISH_SOURCE_HASH=... make test-memory-source  Qualify an activated real-pack source map route'
	@echo '  AOE_GEODATA_CACHE=... make test-creator-source  Qualify overview and detailed source maps across restart'
	@echo '  AOE_GEODATA_CACHE=... make test-geographic-matrix  Prepare and verify fixed overview regions'
	@echo '  make test-geographic-visuals  Capture water and Alpine packages on WebGPU and Canvas'
	@echo '  make test-geographic-visuals-unit  Test page evidence and package-directory config'
	@echo '  make test-wasm       Execute wasm-bindgen tests in pinned Chromium'
	@echo '  make fuzz-smoke      Run bounded parser fuzzing with pinned nightly Rust'
	@echo '  make fuzz-nightly    Run longer parser fuzzing campaigns'
	@echo '  make mutation-nightly  Check critical gate and comparator mutations'
	@echo '  make perf-smoke      Run synthetic smoke workload with real protocol clients'
	@echo '  make perf-ci         Run target workloads and required comparisons'
	@echo '  make perf-full       Add population and sparse-world scaling workloads'
	@echo '  make perf-pressure   Schedule independent network offers and camera changes'
	@echo '  make perf-stress     Characterize explicit beyond-target overload'
	@echo '  make perf-hardware-check  Check dedicated-machine environment and timing samples'
	@echo '  make perf-soak-10    Repeat target network lifecycle for 10 minutes'
	@echo '  make perf-soak-30    Repeat target network lifecycle for 30 minutes'
	@echo '  make perf-instructions Run pinned Gungraun/Callgrind kernels'
	@echo '  make perf-timing    Collect informational Criterion microbenchmarks'
	@echo '  make perf-baseline-propose Write a reviewable baseline proposal'
	@echo '  make perf-comment   Post the informational CI performance comment on a pull request'
	@echo '  make qa-validate     Validate reports/qa/session.json'
	@echo '  make qa-serve        Serve restricted MCP browser tools on stdio'
	@echo '  make release-build  Build local runtime images from clean dev with evidence'
	@echo '  AOE_RELEASE_MANIFEST=... make release-verify  Verify a local release'
	@echo '  AOE_RELEASE_CANDIDATE=... AOE_RELEASE_PREVIOUS=... make release-rehearse'
	@echo '  make repo-policy-check  Audit GitHub branch and merge settings'

.cache/cargo:
	@mkdir -p .cache/cargo

tools: .cache/cargo
	@$(call IMAGE_BUILD,rust-tools,$(TOOL_IMAGE))

browser-tools:
	@$(call IMAGE_BUILD,browser-tools,$(BROWSER_IMAGE))

analysis-tools: tools
	@$(call IMAGE_BUILD,analysis,$(ANALYSIS_IMAGE))

policy-tools: tools
	@$(call IMAGE_BUILD,policy,$(POLICY_IMAGE))

coverage-tools: tools
	@$(call IMAGE_BUILD,coverage,$(COVERAGE_IMAGE))

fuzz-tools: tools
	@$(call IMAGE_BUILD,fuzz,$(FUZZ_IMAGE))

mutation-tools:
	@$(call IMAGE_BUILD,mutation,$(MUTATION_IMAGE))

orchestrator-tools: tools
	@$(call IMAGE_BUILD,orchestrator,$(ORCH_IMAGE))

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

# Stops the containers a host suspend left running (their gap no longer matches the machine's).
recover:
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- recover

include make/agents.mk make/ship.mk make/qualification.mk make/heavy.mk

structure-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- structure-check

architecture-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- architecture-check

.PHONY: contracts-check review-subject
contracts-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- contracts-check

REVIEW_QUOTE = '$(subst ','"'"',$(1))'
REVIEW_COMMA := ,
# Quote complete CSV fields for Docker binds; colons stay ordinary path data.
MOUNT_QUOTE = '$(subst ','"'"',$(1))'
MOUNT_FIELD = "$(subst ","",$(1))"
BIND_MOUNT = --mount $(call MOUNT_QUOTE,type=bind$(COMMA)$(call MOUNT_FIELD,source=$(1))$(COMMA)$(call MOUNT_FIELD,target=$(2))$(if $(3),$(COMMA)readonly))
REVIEW_RUN = docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GIT_INDEX_FILE $(ROOT_MOUNTS) --mount $(call REVIEW_QUOTE,type=bind$(REVIEW_COMMA)source=$(REVIEW_OUTPUT)$(REVIEW_COMMA)target=$(REVIEW_OUTPUT)) $(if $(REVIEW_INPUT),--mount $(call REVIEW_QUOTE,type=bind$(REVIEW_COMMA)source=$(REVIEW_INPUT)$(REVIEW_COMMA)target=$(REVIEW_INPUT)$(REVIEW_COMMA)readonly)) -w $(ROOT) $(TOOL_IMAGE)
review-subject: tools
	@$(if $(filter undefined,$(origin GIT_INDEX_FILE)),:,$(error review-subject Make wrapper does not support defined GIT_INDEX_FILE; use the CLI with an explicitly mounted intentional index))
	@$(if $(findstring $(REVIEW_COMMA),$(REVIEW_OUTPUT)$(REVIEW_INPUT)),$(error review-subject Docker mount paths must not contain CSV commas),:)
	@$(REVIEW_RUN) cargo run --locked -p aoe-harness -- review-subject --base $(call REVIEW_QUOTE,$(REVIEW_BASE)) $(if $(REVIEW_INDEX),--index) $(if $(REVIEW_CANDIDATE),--candidate $(call REVIEW_QUOTE,$(REVIEW_CANDIDATE))) --output $(call REVIEW_QUOTE,$(REVIEW_OUTPUT)) $(if $(REVIEW_INPUT),--review $(call REVIEW_QUOTE,$(REVIEW_INPUT)))

docs-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- docs-check

fmt-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fmt-check

fmt:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fmt

lint:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- lint

deny: policy-tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(POLICY_IMAGE) cargo deny --locked check

test-unit: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-unit -- make --no-print-directory test-unit-steps

.PHONY: test-harness gates-docs gate-plan scope-check gate-run

gates-docs:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- gates-docs

gate-plan:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- gate-plan pr

scope-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- scope-check

gate-run: harness-gate-run-path-check
	@$(HARNESS_LOCAL_RUN) cargo run --locked -p aoe-harness -- gate-run --cadence $(HARNESS_CADENCE) --scope $(HARNESS_SCOPE) --output $(HARNESS_EVIDENCE_REAL)

.PHONY: harness-evidence-path-check harness-gate-run-path-check
harness-evidence-path-check:
	@$(call HARNESS_VALIDATE_EVIDENCE_PATH) true

harness-gate-run-path-check: harness-evidence-path-check
	@$(if $(strip $(HARNESS_ACTIVE_DOCKER_SOCKET_REAL)),true,$(error gate-run requires a resolvable local Unix Docker socket))

geodata-bootstrap:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- bootstrap

geodata-verify:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- verify

map-estimate:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- map-estimate

.PHONY: geodata-test

test-wasm: browser-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-wasm -- make --no-print-directory test-wasm-steps

coverage-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- coverage-check

ci-select:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_BASE_SHA -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-select

ci-dispatch-status: orchestrator-tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GH_TOKEN -e JOB_STATUS -e GITHUB_ACTIONS -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e GITHUB_SHA -e GITHUB_REPOSITORY -e GITHUB_RUN_ID -e GITHUB_SERVER_URL $(ROOT_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- ci-dispatch-status

ci-check:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_BASE_SHA -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e AOE_SELECTION_JSON -e AOE_JOB_RESULTS_JSON $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-check

perf-baseline-propose: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-baseline-propose

# The pull request's informational performance comment (item 12 of #388).
# PR and run arrive through the environment; GITHUB_TOKEN authorizes gh.
# PERF_COMMENT_PRINT=1 prints the comment instead of posting it.
perf-comment:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GITHUB_TOKEN -e AOE_PR_NUMBER -e AOE_RUN_ID -e GITHUB_RUN_ID -e PERF_COMMENT_PRINT $(ROOT_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- perf-comment

perf-hardware-check: tools
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-hardware-check

qa-validate:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- qa-validate

qa-serve:
	@$(MAKE) --no-print-directory orchestrator-tools browser-deps 1>&2
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) -i --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_QA_BASE_URL $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- qa-serve

pre-commit:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- pre-commit

preflight: deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight

# The two halves of preflight: CI's static job runs them as parallel legs.
preflight-checks: deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight --part checks

preflight-unit:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight --part unit

# The pre-push hook: skip the preflight `make ship` just passed at this exact
# commit (AOE_SHIP_EVIDENCE, checked in Rust), otherwise run it.
pre-push:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- prepush-evidence || $(MAKE) --no-print-directory preflight

.PHONY: test-memory-source
test-memory-source: build-wasm browser-deps slot-bootstrap
	@$(SLOT_RUN) test-memory-source -- make --no-print-directory test-memory-source-steps

test-memory-source-steps:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --ipc host --user $(UID):$(GID) -e HOME=$(ROOT)/.cache/browser-home -e AOE_POLISH_SOURCE_URL -e AOE_POLISH_SOURCE_HASH -e AOE_POLISH_SOURCE_REQUIRED=1 $(ROOT_MOUNTS) -w $(ROOT)/browser $(BROWSER_IMAGE) xvfb-run -a npx playwright test tests/memory/source-route.shared-surfaces.spec.ts

test-creator-source: build-wasm browser-deps orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-creator-source -- make --no-print-directory test-creator-source-steps

test-creator-source-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_GEODATA_CACHE -e AOE_ASSET_PACK $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-creator-source

test-geographic-matrix: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-geographic-matrix -- make --no-print-directory test-geographic-matrix-steps

test-geographic-matrix-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_GEODATA_CACHE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-geographic-matrix

test-geographic-visuals: build-wasm browser-deps orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-geographic-visuals -- make --no-print-directory test-geographic-visuals-steps

test-geographic-visuals-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_ASSET_PACK -e AOE_GEODATA_CACHE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-geographic-visuals

dev: build-wasm orchestrator-tools dev-build
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

# The owner's licensed install is mounted read-only; the pack, previews and table go to local-assets/.
DE_DIR ?= $(HOME)/.steam/steam/steamapps/common/AoE2DE
export DE_DIR
assets-import-de:
	@$(subst $(TOOL_IMAGE),$(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo run --locked -p aoe-harness -- assets import-de /de-install

assets-verify:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets verify

assets-dat-table:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets dat-table


release-publish: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e GITHUB_REPOSITORY -e GITHUB_SHA -e GITHUB_REF -e GITHUB_ACTOR -e GITHUB_TOKEN -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-publish

release-source-check: tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_RELEASE_BRANCH -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- release-source-check

release-main-source-check: tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- release-main-source-check

release-verify-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e AOE_RELEASE_SOURCE_SHA -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-verify-published

release-rehearse-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e AOE_PREVIOUS_MANIFEST -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-rehearse-published

release-smoke-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-smoke-published

release-verify: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_RELEASE_MANIFEST $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-verify

release-rehearse: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_RELEASE_CANDIDATE -e AOE_RELEASE_PREVIOUS $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-rehearse

repo-policy-check:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GITHUB_TOKEN $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- repo-policy-check
