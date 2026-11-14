geodata-bootstrap:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- bootstrap

geodata-verify:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- verify

map-estimate:
	@$(GEODATA_RUN) cargo run --locked -p aoe-geodata --bin aoe-map-worker -- map-estimate

.PHONY: geodata-test

# Resolve the ignored link on the host so local fixtures are visible in Docker.
FIXTURES_REPLAY := $(realpath $(ROOT)/.cache/dev-replay)
.PHONY: check-fixtures check-fixtures-report
# check-fixtures fails on a fixture that does not load; the ship's report form only warns.
check-fixtures check-fixtures-report: job-bootstrap
	@$(RESOURCE_RUN) docker run $(JOB_ENV) $(RESOURCE_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) $(if $(FIXTURES_REPLAY),$(call BIND_MOUNT,$(FIXTURES_REPLAY),$(FIXTURES_REPLAY))) -w $(ROOT) $(TOOL_IMAGE) $(RESOURCE_OBSERVER) cargo run --locked -p aoe-harness -- check-fixtures $(if $(filter check-fixtures-report,$@),--report)

map-generate: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-generate -- make --no-print-directory map-generate-steps

map-generate-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-generate

map-generate-detailed: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-generate-detailed -- make --no-print-directory map-generate-detailed-steps

map-generate-detailed-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-generate-detailed

map-verify: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-verify -- make --no-print-directory map-verify-steps

map-verify-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-verify

map-perf: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-perf -- make --no-print-directory map-perf-steps

map-perf-steps:
	@$(GEODATA_RUN) cargo run --release --locked -p aoe-geodata --bin aoe-map-worker -- map-perf

map-source-qualify: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-source-qualify -- make --no-print-directory map-source-qualify-steps

map-source-qualify-steps:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-qualify --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' $(SOURCE_QUAL_GEOGRAPHIC_ARGS) $(SOURCE_QUAL_SCALE_ARGS)

map-source-scale-qualify: orchestrator-tools job-bootstrap
	@$(JOB_RUN) map-source-scale-qualify -- make --no-print-directory map-source-scale-qualify-steps

map-source-scale-qualify-steps:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-qualify --scale-only --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' $(SOURCE_QUAL_SCALE_ARGS)
