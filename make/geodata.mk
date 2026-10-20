geodata-bootstrap:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- bootstrap

geodata-verify:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- verify

map-estimate:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- map-estimate

.PHONY: geodata-test

map-generate: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-generate -- make --no-print-directory map-generate-steps

map-generate-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-generate

map-generate-detailed: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-generate-detailed -- make --no-print-directory map-generate-detailed-steps

map-generate-detailed-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-generate-detailed

map-verify: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-verify -- make --no-print-directory map-verify-steps

map-verify-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-verify

map-perf: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-perf -- make --no-print-directory map-perf-steps

map-perf-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-perf

map-source-qualify: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-source-qualify -- make --no-print-directory map-source-qualify-steps

map-source-qualify-steps:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-qualify --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' $(SOURCE_QUAL_GEOGRAPHIC_ARGS) $(SOURCE_QUAL_SCALE_ARGS)

map-source-scale-qualify: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-source-scale-qualify -- make --no-print-directory map-source-scale-qualify-steps

map-source-scale-qualify-steps:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-qualify --scale-only --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' $(SOURCE_QUAL_SCALE_ARGS)
