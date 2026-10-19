# Agent-runtime targets (docs/agent-runtimes.md). Logic lives in aoe-harness.

# Agent hook judge, built by .agents/hooks/harness.sh from origin/dev (or this checkout while bootstrapping).
AGENT_HOOK_HOME := $(GIT_COMMON)/aoe-agent-hook
.PHONY: agent-hook-build
agent-hook-build:
	@test -n '$(AGENT_HOOK_OUTPUT)' || { echo 'run .agents/hooks/harness.sh build' >&2; exit 2; }
	@$(if $(AGENT_HOOK_REVISION),rm -rf '$(AGENT_HOOK_HOME)/source' && mkdir -p '$(AGENT_HOOK_HOME)/source' && git archive '$(AGENT_HOOK_REVISION)' | tar -x -C '$(AGENT_HOOK_HOME)/source',true)
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(if $(AGENT_HOOK_REVISION),$(AGENT_HOOK_HOME)/source,$(ROOT)) $(TOOL_IMAGE) cargo build --locked -p aoe-harness --target-dir $(AGENT_HOOK_HOME)/target
	@install -D -m 0755 '$(AGENT_HOOK_HOME)/target/debug/aoe-harness' '$(AGENT_HOOK_OUTPUT)'
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -w $(if $(AGENT_HOOK_REVISION),$(AGENT_HOOK_HOME)/source,$(ROOT)) $(TOOL_IMAGE) '$(AGENT_HOOK_OUTPUT)' agent-prune --home '$(AGENT_HOOK_HOME)' --keep '$(notdir $(patsubst %/,%,$(dir $(AGENT_HOOK_OUTPUT))))'

# Run a runtime's own CLI headless as a launched implementer and check that its
# tool calls reach the judge: RUNTIME=claude|codex|dsh|pi.
.PHONY: agent-smoke
agent-smoke:
	@test -n '$(RUNTIME)' || { echo 'usage: make agent-smoke RUNTIME=claude|codex|dsh|pi' >&2; exit 2; }
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' agent-smoke --runtime '$(RUNTIME)'

# Focused native coverage; counters and compiler outputs stay in target/.
.PHONY: test-placement-coverage

test-placement:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-placement

hooks-install:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-install

hooks-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-check

# Docker creates missing nested bind destinations as root in the checkout.
# This bootstrap must run as the caller, before any cache-using container.
ifneq ($(strip $(CACHE_MOUNTS)),)
.EXTRA_PREREQS += cache-mount-points
.PHONY: cache-mount-points
cache-mount-points:
	@mkdir -p $(if $(filter $(GIT_COMMON)/aoe-cargo,$(HARNESS_CARGO_CACHE)),'$(HARNESS_CARGO_CACHE)') $(if $(filter-out $(ROOT)/.cache/cargo,$(HARNESS_CARGO_CACHE)),'$(ROOT)/.cache/cargo') $(if $(filter-out $(ROOT)/target,$(HARNESS_TARGET_CACHE)),'$(ROOT)/target')
endif

override HARNESS_TEST_FILTER := $(value HARNESS_TEST_FILTER)
export HARNESS_TEST_FILTER
.PHONY: test-harness-focused
