SHELL := /bin/sh
.DEFAULT_GOAL := help
ROOT := $(abspath .)
UID := $(shell id -u)
GID := $(shell id -g)
TOOL_IMAGE := aoeworld/rust-tools:1.99.1
BROWSER_IMAGE := aoeworld/browser-tools:1.64.0
ORCH_IMAGE := aoeworld/orchestrator:1.99.1
ANALYSIS_IMAGE := aoeworld/analysis:0.19.4
POLICY_IMAGE := aoeworld/policy:0.20.2
COVERAGE_IMAGE := aoeworld/coverage:0.9.1
FUZZ_IMAGE := aoeworld/fuzz:nightly-2026-09-01-0.13.2
MUTATION_IMAGE := aoeworld/mutation:27.1.0
SDK_IMAGE := aoeworld/sdk-python:1.15.0
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
HARNESS_CARGO_CACHE ?= $(GIT_COMMON)/aoe-cargo
HARNESS_TARGET_CACHE ?= $(ROOT)/target
# Expose the cache at its host path too: harness commands start sibling containers.
CACHE_MOUNTS := $(if $(filter-out $(ROOT)/.cache/cargo,$(HARNESS_CARGO_CACHE)),$(call BIND_MOUNT,$(HARNESS_CARGO_CACHE),$(ROOT)/.cache/cargo) $(call BIND_MOUNT,$(HARNESS_CARGO_CACHE),$(HARNESS_CARGO_CACHE))) $(if $(filter-out $(ROOT)/target,$(HARNESS_TARGET_CACHE)),$(call BIND_MOUNT,$(HARNESS_TARGET_CACHE),$(ROOT)/target))
HEAVY_ENV := -e $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE)) -e GITHUB_ACTIONS -e CI -e AOE_DISK_FLOOR_GIB -e AOE_TEST_SELECT -e AOE_TEST_ALL -e AOE_HEAVY_SLOT -e AOE_SLOT_CPUS -e AOE_HEAVY_DIR -e AOE_HEAVY_SLOTS -e AOE_HEAVY_WAIT_MINUTES
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
DOCKER_RUN := \
  docker run --rm --init \
  --label aoe.harness-boot-gap=$(AOE_HARNESS_BOOT_GAP) \
  $(HEAVY_ENV) \
  $(SLOT_CAP) \
  --user $(UID):$(GID) \
  -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) \
  -e GIT_INDEX_FILE \
  -e RUST_TEST_THREADS \
  -e NEXTEST_TEST_THREADS \
  $(TMP_MOUNT) \
  -e AOE_SHIP_EVIDENCE \
  $(ROOT_MOUNTS) \
  -w $(ROOT) \
  $(TOOL_IMAGE)
.PHONY: \
  help \
  bootstrap \
  analysis-tools \
  policy-tools \
  coverage-tools \
  fuzz-tools \
  mutation-tools \
  browser-tools \
  orchestrator-tools \
  browser-deps \
  browser-check
.PHONY: \
  test-wasm \
  test-e2e \
  test-creator-source \
  test-geographic-matrix \
  test-placement \
  deny \
  test-unit \
  test-source-qualification \
  test-exhaustive \
  coverage \
  coverage-check \
  test-geographic-visuals \
  test-geographic-visuals-unit
.PHONY: \
  doctor \
  hooks-install \
  hooks-check \
  structure-check \
  architecture-check \
  docs-check \
  fmt \
  fmt-check \
  lint \
  qa-validate \
  qa-serve \
  repo-policy-check
.PHONY: \
  tools \
  ci-select \
  ci-check \
  ci-dispatch-status \
  pre-commit \
  pre-push \
  preflight \
  preflight-checks \
  preflight-unit \
  build \
  build-wasm \
  dev \
  down \
  status \
  logs \
  recover
GEODATA_PATHS := $(AOE_GEODATA_CACHE) $(AOE_MAP_REQUEST) $(AOE_MAP_PACKAGE)
GEODATA_EXTERNAL_DIRS := $(filter-out $(ROOT) $(ROOT)/%,$(sort $(foreach path,$(filter /%,$(GEODATA_PATHS)),$(patsubst %/,%,$(dir $(path))))))
GEODATA_MOUNTS := $(foreach directory,$(GEODATA_EXTERNAL_DIRS),-v $(directory):$(directory))
GEODATA_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_GEODATA_CACHE -e AOE_MAP_REQUEST -e AOE_MAP_PACKAGE -e AOE_MAP_SAMPLES -e AOE_MAP_DEM_RESOLUTION -e AOE_MAP_HYDROLOGY_MODE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
SOURCE_QUAL_PATHS := $(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY) $(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY)
SOURCE_QUAL_EXTERNAL_DIRS := $(filter-out $(ROOT) $(ROOT)/%,$(sort $(foreach path,$(filter /%,$(SOURCE_QUAL_PATHS)),$(patsubst %/,%,$(dir $(path))))))
SOURCE_QUAL_MOUNTS := $(foreach directory,$(SOURCE_QUAL_EXTERNAL_DIRS),-v $(directory):$(directory))
SOURCE_QUAL_CARGO_HOME ?= $(HARNESS_CARGO_CACHE)
SOURCE_QUAL_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(SOURCE_QUAL_CARGO_HOME)) $(ROOT_MOUNTS) $(SOURCE_QUAL_MOUNTS) -w $(ROOT) $(TOOL_IMAGE)
SOURCE_QUAL_GEOGRAPHIC_ARGS := $(if $(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY),--geographic-package-directory '$(AOE_SOURCE_QUAL_GEOGRAPHIC_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_GEOGRAPHIC_CONTENT_HASH),--geographic-content-hash '$(AOE_SOURCE_QUAL_GEOGRAPHIC_CONTENT_HASH)')
SOURCE_QUAL_SCALE_ARGS := $(if $(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY),--scale-512-package-directory '$(AOE_SOURCE_QUAL_512_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_512_CONTENT_HASH),--scale-512-content-hash '$(AOE_SOURCE_QUAL_512_CONTENT_HASH)') $(if $(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY),--scale-16384-package-directory '$(AOE_SOURCE_QUAL_16384_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_16384_CONTENT_HASH),--scale-16384-content-hash '$(AOE_SOURCE_QUAL_16384_CONTENT_HASH)') $(if $(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY),--scale-262144-package-directory '$(AOE_SOURCE_QUAL_262144_PACKAGE_DIRECTORY)') $(if $(AOE_SOURCE_QUAL_262144_CONTENT_HASH),--scale-262144-content-hash '$(AOE_SOURCE_QUAL_262144_CONTENT_HASH)')
BROWSER_RUN := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --ipc host --user $(UID):$(GID) -e HOME=$(ROOT)/.cache/browser-home $(ROOT_MOUNTS) -w $(ROOT)/browser $(BROWSER_IMAGE)
DEV_ORCH_BASE := docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_SCENARIO -e AOE_ASSET_PACK -e AOE_OPEN_MAP_PACKAGE -e AOE_FOG_DISABLED -e AOE_FOG_SHOWCASE -e AOE_MATCH_TOKEN -e AOE_ARENA -e AOE_PRESET_MAPS -e DEV_PORT $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT)
DEV_ORCH_RUN := $(DEV_ORCH_BASE) $(ORCH_IMAGE)
HARNESS_EVIDENCE_DIR ?= /tmp/aoeworld-harness-evidence
HARNESS_CADENCE ?= edit
HARNESS_SCOPE ?= working
override HARNESS_LOCAL_RUN = docker run --rm --init $(HEAVY_ENV) -e $(call MOUNT_QUOTE,HARNESS_CARGO_CACHE=$(HARNESS_CARGO_CACHE_REAL)) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g $(HARNESS_ACTIVE_DOCKER_SOCKET_REAL)) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE_REAL)) -e AOE_SCENARIO -e AOE_ASSET_PACK $(HEAVY_DIR_MOUNT) -v $(HARNESS_ROOT_REAL):$(HARNESS_ROOT_REAL) $(if $(HARNESS_GIT_EXTERNAL_REAL),-v $(HARNESS_GIT_EXTERNAL_REAL):$(HARNESS_GIT_EXTERNAL_REAL)) $(if $(HARNESS_CARGO_CACHE_MOUNT_SOURCE),-v $(HARNESS_CARGO_CACHE_REAL):$(HARNESS_ROOT_REAL)/.cache/cargo -v $(HARNESS_CARGO_CACHE_REAL):$(HARNESS_CARGO_CACHE_REAL)) $(if $(HARNESS_TARGET_CACHE_MOUNT_SOURCE),-v $(HARNESS_TARGET_CACHE_REAL):$(HARNESS_ROOT_REAL)/target) -v $(HARNESS_ACTIVE_DOCKER_SOCKET_REAL):/var/run/docker.sock -w $(HARNESS_ROOT_REAL) -v $(HARNESS_EVIDENCE_REAL):$(HARNESS_EVIDENCE_REAL) $(ORCH_IMAGE)
override HARNESS_ROOT_REAL := $(realpath $(ROOT))
override HARNESS_GIT_EXTERNAL_REAL := $(realpath $(GIT_EXTERNAL))
override HARNESS_CARGO_CACHE_MOUNT_SOURCE := $(filter-out $(ROOT)/.cache/cargo,$(HARNESS_CARGO_CACHE))
override HARNESS_TARGET_CACHE_MOUNT_SOURCE := $(filter-out $(ROOT)/target,$(HARNESS_TARGET_CACHE))
override HARNESS_CARGO_CACHE_REAL = $(realpath $(HARNESS_CARGO_CACHE))
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
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- recover

include make/agents.mk make/ship.mk make/qualification.mk make/heavy.mk make/phony.mk make/help.mk make/assets.mk make/geodata.mk make/release.mk make/sdk.mk

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
REVIEW_RUN = docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GIT_INDEX_FILE $(ROOT_MOUNTS) --mount $(call REVIEW_QUOTE,type=bind$(REVIEW_COMMA)source=$(REVIEW_OUTPUT)$(REVIEW_COMMA)target=$(REVIEW_OUTPUT)) $(if $(REVIEW_INPUT),--mount $(call REVIEW_QUOTE,type=bind$(REVIEW_COMMA)source=$(REVIEW_INPUT)$(REVIEW_COMMA)target=$(REVIEW_INPUT)$(REVIEW_COMMA)readonly)) -w $(ROOT) $(TOOL_IMAGE)
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
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(ROOT) $(POLICY_IMAGE) cargo run --locked -p aoe-harness -- deny

test-unit: disk-precheck orchestrator-tools slot-bootstrap
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

test-wasm: disk-precheck browser-tools orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-wasm -- make --no-print-directory test-wasm-steps

coverage-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- coverage-check

ci-select:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_BASE_SHA -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-select

ci-dispatch-status: orchestrator-tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GH_TOKEN -e JOB_STATUS -e GITHUB_ACTIONS -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e GITHUB_SHA -e GITHUB_REPOSITORY -e GITHUB_RUN_ID -e GITHUB_SERVER_URL $(ROOT_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- ci-dispatch-status

ci-check:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_BASE_SHA -e GITHUB_EVENT_NAME -e GITHUB_REF_NAME -e AOE_SELECTION_JSON -e AOE_JOB_RESULTS_JSON $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- ci-check

perf-baseline-propose: perf-instructions perf-wasm-size
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-baseline-propose

# The pull request's informational performance comment (item 12 of #388).
# PR and run arrive through the environment; GITHUB_TOKEN authorizes gh.
# PERF_COMMENT_PRINT=1 prints the comment instead of posting it.
perf-comment:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --network host --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GITHUB_TOKEN -e AOE_PR_NUMBER -e AOE_RUN_ID -e GITHUB_RUN_ID -e PERF_COMMENT_PRINT $(ROOT_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- perf-comment

perf-hardware-check: tools
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- perf-hardware-check

qa-validate:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- qa-validate

qa-serve:
	@$(MAKE) --no-print-directory orchestrator-tools browser-deps 1>&2
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) -i --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_QA_BASE_URL $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- qa-serve

pre-commit:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- pre-commit

# CI's static job runs these two halves as parallel legs.
preflight-checks: disk-precheck
	@$(MAKE) --no-print-directory deny build-wasm
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- preflight --part checks

preflight-unit: disk-precheck
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
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_GEODATA_CACHE -e AOE_ASSET_PACK $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-creator-source

test-geographic-matrix: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-geographic-matrix -- make --no-print-directory test-geographic-matrix-steps

test-geographic-matrix-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_GEODATA_CACHE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-geographic-matrix

test-geographic-visuals: build-wasm browser-deps orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) test-geographic-visuals -- make --no-print-directory test-geographic-visuals-steps

test-geographic-visuals-steps:
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-geodata --bin aoe-map-worker
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_ASSET_PACK -e AOE_GEODATA_CACHE $(ROOT_MOUNTS) $(GEODATA_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- test-geographic-visuals

dev: build-wasm orchestrator-tools dev-build
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev start

.PHONY: dev-start
# Restart a build already made by `make dev`; does not compile the game.
dev-start: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev start

down: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev down

status: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev status

logs: orchestrator-tools
	@$(DEV_ORCH_RUN) cargo run --locked -p aoe-harness -- dev logs

repo-policy-check:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GITHUB_TOKEN $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- repo-policy-check

.PHONY: cargo-fetch
cargo-fetch:
	@$(DOCKER_RUN) cargo fetch --locked --config net.offline=false
