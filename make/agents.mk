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
