# Publishing (docs/shipping.md). `make ship` is the only way agents publish:
# preflight gates at the exact commit, evidence, push, pull request.
.PHONY: ship ship-status
ship:
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' ship $(if $(SHIP_TITLE),--title '$(subst ','"'"',$(SHIP_TITLE))') $(if $(SHIP_BODY),--body-file '$(SHIP_BODY)') $(if $(filter 1,$(SHIP_FORCE)),--force)

ship-status:
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' ship-status
