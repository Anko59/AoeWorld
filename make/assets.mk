assets-inspect:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets inspect

assets-import:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets import

# The owner's licensed install is mounted read-only; the pack, previews and table go to local-assets/.
DE_DIR ?= $(HOME)/.steam/steam/steamapps/common/AoE2DE
export DE_DIR
assets-import-de:
	@$(subst $(TOOL_IMAGE),$(call BIND_MOUNT,$(DE_DIR),/de-install,1) $(TOOL_IMAGE),$(DOCKER_RUN)) cargo run --locked -p aoe-harness -- assets import-de /de-install $(if $(AOE_ASSET_PACK),$(call MOUNT_QUOTE,$(AOE_ASSET_PACK)))

assets-verify:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets verify $(if $(AOE_ASSET_PACK),$(call MOUNT_QUOTE,$(AOE_ASSET_PACK)))

assets-dat-table:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- assets dat-table

.PHONY: fishing-scenario
fishing-scenario:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- fishing-scenario
