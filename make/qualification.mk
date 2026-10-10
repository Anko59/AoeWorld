# Source-backed qualification of one external package (docs/geodata/landscape).
.PHONY: map-country-probe
map-country-probe:
	@$(SOURCE_QUAL_RUN) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)'

.PHONY: test-country-source
test-country-source: build-wasm browser-deps orchestrator-tools
	@$(DOCKER_RUN) cargo build --locked --release -p aoe-server
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e AOE_ASSET_PACK $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --release --locked -p aoe-harness -- source-country-probe --package-directory '$(AOE_SOURCE_QUAL_PACKAGE_DIRECTORY)' --content-hash '$(AOE_SOURCE_QUAL_CONTENT_HASH)' --browser
