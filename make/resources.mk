# Independent std-only observer: no workspace bootstrap or machine-wide build lock.
RESOURCE_SOURCE := crates/harness/src/resources
.PHONY: resource-measure test-resources
$(RESOURCE_BINARY): $(RESOURCE_SOURCE)/launcher.rs $(RESOURCE_SOURCE)/measure.rs
	@mkdir -p .cache
	@docker run --rm --init --memory=256m --memory-swap=256m --cpu-shares=1024 --user $(UID):$(GID) $(call BIND_MOUNT,$(ROOT),$(ROOT)) -w $(ROOT) $(TOOL_IMAGE) rustc --edition=2024 $(RESOURCE_SOURCE)/launcher.rs -o $(RESOURCE_BINARY)

resource-measure: $(RESOURCE_BINARY)
	@$(RESOURCE_BINARY) $(RESOURCE_MEASURE_ARGS)

test-resources:
	@mkdir -p .cache
	@docker run --rm --init --memory=256m --memory-swap=256m --cpu-shares=1024 --user $(UID):$(GID) $(call BIND_MOUNT,$(ROOT),$(ROOT)) -w $(ROOT) $(TOOL_IMAGE) rustc --edition=2024 --test $(RESOURCE_SOURCE)/launcher.rs -o .cache/resource-tests
	@.cache/resource-tests
