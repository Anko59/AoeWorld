# Python bindings of the AI SDK (docs/ai-sdk.md). The bindings live outside the
# Cargo workspace (crates/sdk-python) because they need a Python toolchain; their
# image extends the pinned tools image with pinned maturin and pytest.
.PHONY: sdk-python-tools sdk-python sdk-python-steps sdk-python-test sdk-python-test-steps
sdk-python-tools: tools
	@$(call IMAGE_BUILD,sdk-python,$(SDK_IMAGE))

SDK_RUN = \
  docker run --rm --init \
  $(HEAVY_ENV) \
  $(SLOT_CAP) \
  --user $(UID):$(GID) \
  -e HOME=/tmp \
  -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) \
  $(ROOT_MOUNTS) \
  -w $(ROOT) \
  $(SDK_IMAGE)

# Builds the aoeworld wheel into target/wheels.
sdk-python: sdk-python-tools slot-bootstrap
	@$(SLOT_RUN) sdk-python -- make --no-print-directory sdk-python-steps

sdk-python-steps:
	@$(SDK_RUN) cargo run --locked -p aoe-harness -- sdk-python build

# Builds the server and a wheel, then runs pytest for the bindings and the
# example AI against a local server.
sdk-python-test: sdk-python-tools slot-bootstrap
	@$(SLOT_RUN) sdk-python-test -- make --no-print-directory sdk-python-test-steps

sdk-python-test-steps:
	@$(SDK_RUN) cargo run --locked -p aoe-harness -- sdk-python test
