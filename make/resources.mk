# Independent std-only observer: no workspace bootstrap or machine-wide build lock.
RESOURCE_SOURCE := crates/harness/src/resources
.PHONY: test-resources
.cache/resource-runner: $(RESOURCE_SOURCE)/launcher.rs $(RESOURCE_SOURCE)/observer.rs $(RESOURCE_SOURCE)/policy.rs gates/container.args
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
