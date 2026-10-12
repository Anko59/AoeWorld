# Agent-runtime targets (docs/agent-runtimes.md). Logic lives in aoe-harness.

# Agent hook judge, built by .agents/hooks/harness.sh from origin/dev (or this checkout while bootstrapping).
AGENT_HOOK_HOME := $(GIT_COMMON)/aoe-agent-hook
.PHONY: agent-hook-build
agent-hook-build:
	@test -n '$(AGENT_HOOK_OUTPUT)' || { echo 'run .agents/hooks/harness.sh build' >&2; exit 2; }
	@$(if $(AGENT_HOOK_REVISION),rm -rf '$(AGENT_HOOK_HOME)/source' && mkdir -p '$(AGENT_HOOK_HOME)/source' && git archive '$(AGENT_HOOK_REVISION)' | tar -x -C '$(AGENT_HOOK_HOME)/source',true)
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(if $(AGENT_HOOK_REVISION),$(AGENT_HOOK_HOME)/source,$(ROOT)) $(TOOL_IMAGE) cargo build --locked -p aoe-harness --target-dir $(AGENT_HOOK_HOME)/target
	@install -D -m 0755 '$(AGENT_HOOK_HOME)/target/debug/aoe-harness' '$(AGENT_HOOK_OUTPUT)' && find '$(AGENT_HOOK_HOME)' -mindepth 1 -maxdepth 1 -type d ! -name target ! -name source -mtime +7 -exec rm -rf {} +

# Run a runtime's own CLI headless as a launched implementer and check that its
# tool calls reach the judge: RUNTIME=claude|codex|dsh|pi.
.PHONY: agent-smoke
agent-smoke:
	@test -n '$(RUNTIME)' || { echo 'usage: make agent-smoke RUNTIME=claude|codex|dsh|pi' >&2; exit 2; }
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' agent-smoke --runtime '$(RUNTIME)'

# Focused native coverage; counters and compiler outputs stay in target/.
.PHONY: test-placement-coverage
test-placement-coverage: coverage-tools
	@mkdir -p reports/placement
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov nextest --locked -p aoe-harness --no-report -E 'test(gates::placement::tests::)'
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(TMP_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov run --locked -p aoe-harness --no-report -- test-placement
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo $(ROOT_MOUNTS) -w $(ROOT) $(COVERAGE_IMAGE) cargo llvm-cov report --lcov --output-path reports/placement/coverage.lcov

test-placement:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- test-placement

hooks-install:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-install

hooks-check:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- hooks-check
