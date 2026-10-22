# Python bindings of the AI SDK (docs/ai-sdk.md). The bindings live outside the
# Cargo workspace (crates/sdk-python) because they need a Python toolchain; their
# image extends the pinned tools image with pinned maturin and pytest.
.PHONY: sdk-python-tools sdk-python sdk-python-steps sdk-python-test sdk-python-test-steps ai-arena ai-arena-steps
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

# The AI arena: seeded matches that run as fast as the machine allows (docs/ai-sdk.md).
# ARENA_SIZE tiles per side, ARENA_BOTS players, ARENA_MINUTES game minutes, ARENA_SEEDS one match each.
ARENA_SIZE ?= 300
ARENA_BOTS ?= 2
ARENA_MINUTES ?= 30
ARENA_SEEDS ?= 1,2,3,4,5
ai-arena: sdk-python-tools slot-bootstrap
	@$(SLOT_RUN) ai-arena -- make --no-print-directory ai-arena-steps ARENA_SIZE=$(ARENA_SIZE) ARENA_BOTS=$(ARENA_BOTS) ARENA_MINUTES=$(ARENA_MINUTES) ARENA_SEEDS=$(ARENA_SEEDS)

ai-arena-steps:
	@$(SDK_RUN) cargo run --locked -p aoe-harness -- sdk-python arena --size $(ARENA_SIZE) --bots $(ARENA_BOTS) --minutes $(ARENA_MINUTES) --seeds $(ARENA_SEEDS)
