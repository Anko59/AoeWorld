assets-inspect:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets inspect

assets-import:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets import

# The owner's licensed install is mounted read-only; the pack, previews and table go to local-assets/.
DE_DIR ?= $(HOME)/.steam/steam/steamapps/common/AoE2DE
export DE_DIR
assets-import-de:
	@$(subst $(TOOL_IMAGE),$(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo run --locked -p aoe-harness -- assets import-de /de-install $(if $(AOE_ASSET_PACK),$(call MOUNT_QUOTE,$(AOE_ASSET_PACK)))

.PHONY: assets-de-reference
assets-de-reference:
	@$(subst $(TOOL_IMAGE),$(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo run --locked -p aoe-harness -- assets de-reference /de-install .cache/tc-shots

assets-verify:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets verify $(if $(AOE_ASSET_PACK),$(call MOUNT_QUOTE,$(AOE_ASSET_PACK)))

assets-dat-table:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets dat-table

.PHONY: fishing-scenario
fishing-scenario:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fishing-scenario
.PHONY: assets-dat-table-de
assets-dat-table-de:
	@$(subst $(TOOL_IMAGE),$(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo run --locked -p aoe-harness -- assets dat-table --de /de-install/resources/_common/dat/empires2_x2_p1.dat
.PHONY: assets-building-preview

BUILDING_PREVIEW_OUT ?= .cache/tc-shots/tc-composite.png
assets-building-preview:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets building-preview '$(AOE_ASSET_PACK)' '$(BUILDING_PREVIEW_OUT)' $(if $(BUILDING_PREVIEW_RECIPE),--recipe '$(BUILDING_PREVIEW_RECIPE)')

.PHONY: building-proof building-proof-run building-proof-build building-proof-build-steps
building-proof: building-proof-build

# Repeat local protocol preparation with the already-built proof tool.
building-proof building-proof-run:
	@$(subst docker run,docker run --network host,$(DOCKER_RUN)) target/debug/examples/building-proof '$(BUILDING_PROOF_REQUEST)'

building-proof-build: slot-bootstrap
	@$(SLOT_RUN) building-proof-build -- make --no-print-directory building-proof-build-steps

building-proof-build-steps:
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness --example building-proof

.PHONY: test-assets-pack test-assets-pack-steps
test-assets-pack: slot-bootstrap
	@$(SLOT_RUN) test-assets-pack -- make --no-print-directory test-assets-pack-steps

test-assets-pack-steps:
	@$(subst $(TOOL_IMAGE),-e $(call MOUNT_QUOTE,AOE_REVIEWED_PACK_MANIFEST=$(ROOT)/$(AOE_ASSET_PACK)/manifest.json) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo nextest run --locked -p aoe-assets --run-ignored only --no-tests fail --success-output immediate -E 'test(original_manifest_all_reviewed_frames_fit_fixed_atlas)'
