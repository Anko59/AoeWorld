# Source-backed qualification of one external package (docs/geodata/landscape).
.PHONY: map-country-probe
map-country-probe:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)'
