# Source-backed qualification of one external package (docs/geodata/landscape).
.PHONY: map-country-probe
map-country-probe: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) map-country-probe -- make --no-print-directory map-country-probe-steps

map-country-probe-steps:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)'

.PHONY: test-country-source
