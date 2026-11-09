# Independent std-only observer: no workspace bootstrap or machine-wide build lock.
RESOURCE_SOURCE := crates/harness/src/resources
.PHONY: test-resources
.cache/resource-runner: $(RESOURCE_SOURCE)/launcher.rs $(RESOURCE_SOURCE)/observer.rs $(RESOURCE_SOURCE)/policy.rs $(RESOURCE_SOURCE)/long.rs $(RESOURCE_SOURCE)/lock.rs gates/container.args
	@$(MAKE) --no-print-directory tools
	@mkdir -p .cache
	@docker run --rm --init --memory=256m --memory-swap=256m --cpu-shares=1024 --user $(UID):$(GID) $(call BIND_MOUNT,$(ROOT),$(ROOT)) -w $(call MOUNT_QUOTE,$(ROOT)) $(TOOL_IMAGE) rustc --edition=2024 $(RESOURCE_SOURCE)/launcher.rs -o .cache/resource-runner

.PHONY: resource-bootstrap
.EXTRA_PREREQS += resource-bootstrap
resource-bootstrap: .cache/resource-runner
resource-bootstrap .cache/resource-runner cache-mount-points help tools .cache/cargo: .EXTRA_PREREQS :=

test-resources:
	@mkdir -p .cache
	@docker run --rm --init --memory=256m --memory-swap=256m --cpu-shares=1024 --user $(UID):$(GID) $(call BIND_MOUNT,$(ROOT),$(ROOT)) -w $(ROOT) $(TOOL_IMAGE) rustc --edition=2024 --test $(RESOURCE_SOURCE)/launcher.rs -o .cache/resource-tests
	@.cache/resource-tests

# A tiny adversarial container, independent of the full browser suite.
.PHONY: test-resource-browser-oom resource-browser-oom-child
.cache/browser-resource-tests: $(RESOURCE_SOURCE)/tests/browser_oom.rs
	@docker run --rm --init --memory=256m --memory-swap=256m --cpu-shares=1024 --user $(UID):$(GID) $(call BIND_MOUNT,$(ROOT),$(ROOT)) -w $(ROOT) $(TOOL_IMAGE) rustc --edition=2024 $(RESOURCE_SOURCE)/tests/browser_oom.rs -o .cache/browser-resource-tests

test-resource-browser-oom: .cache/browser-resource-tests browser-tools
	@.cache/browser-resource-tests

# The Rust test supplies a 128 MiB cap and verifies it before allocating.
resource-browser-oom-child: .cache/browser-resource-tests
	@$(BROWSER_RUN) $(ROOT)/.cache/browser-resource-tests --child

# Probe before even building the launcher; the launcher still owns real admission.
LONG_GOALS := perf-census-instructions coverage preflight preflight-unit test-e2e release-build scale-match scale-match-smoke perf-baseline-propose perf-instructions perf-scale perf-scale-smoke perf-groups perf-groups-instructions perf-groups-profile match-bench perf-smoke perf-late-game perf-late-game-instructions perf-ci perf-full perf-pressure perf-stress perf-soak-10 perf-soak-30
ifneq ($(strip $(filter $(LONG_GOALS),$(MAKECMDGOALS))),)
ifneq ($(strip $(filter-out test-e2e,$(filter $(LONG_GOALS),$(MAKECMDGOALS)))$(if $(strip $(AOE_E2E_SPECS)),, $(filter test-e2e,$(MAKECMDGOALS)))),)
.PHONY: long-precheck
resource-bootstrap .cache/resource-runner: | long-precheck
long-precheck: .EXTRA_PREREQS :=
long-precheck:
	@lock=$(call MOUNT_QUOTE,$(GIT_COMMON)/aoe-ship/long.lock); if [ -f "$$lock" ]; then flock -n -E 75 "$$lock" true || { status=$$?; [ "$$status" = 75 ] || exit "$$status"; { IFS= read -r proof; IFS= read -r holder || :; } < "$$lock"; if [ -n "$$AOE_LONG_JOB" ] && [ "$$proof" = "$$AOE_LONG_JOB" ] && [ "$$AOE_LONG_DIR/long.lock" = "$$lock" ]; then exit 0; fi; echo "long job busy: $${holder:-holder metadata is being recorded}, retry later" >&2; exit 75; }; fi
endif
endif
