release-build: orchestrator-tools slot-bootstrap
	@$(SLOT_RUN) release-build -- make --no-print-directory release-build-steps

release-build-steps:
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-build

release-publish: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e GITHUB_REPOSITORY -e GITHUB_SHA -e GITHUB_REF -e GITHUB_ACTOR -e GITHUB_TOKEN -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-publish

release-source-check: tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_RELEASE_BRANCH -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- release-source-check

release-main-source-check: tools
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- release-main-source-check

release-verify-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e AOE_RELEASE_SOURCE_SHA -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN -e GITHUB_OUTPUT $(GITHUB_OUTPUT_MOUNT) $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-verify-published

release-rehearse-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e AOE_PREVIOUS_MANIFEST -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-rehearse-published

release-smoke-published: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e DOCKER_CONFIG=/tmp/aoeworld-docker-auth -e AOE_RELEASE_BRANCH -e GITHUB_REPOSITORY -e GITHUB_ACTOR -e GITHUB_TOKEN $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-smoke-published

release-verify: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_RELEASE_MANIFEST $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-verify

release-rehearse: orchestrator-tools
	@docker run --rm --init $(HEAVY_ENV) $(SLOT_CAP) --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e AOE_RELEASE_CANDIDATE -e AOE_RELEASE_PREVIOUS $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- release-rehearse
