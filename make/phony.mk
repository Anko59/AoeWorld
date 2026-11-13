# Phony release, asset and import targets, one per line (moved from the Makefile to keep it
# within 500 lines).
.PHONY: \
  assets-inspect \
  assets-import \
  assets-verify \
  release-build \
  release-publish \
  release-source-check \
  release-main-source-check \
  release-verify-published \
  release-rehearse \
  release-smoke-published \
  release-verify \
  release-rehearse-published
.PHONY: \
  assets-import-de \
  assets-series-art \
  assets-dat-table

# Formats browser tooling with the pinned formatter.
.PHONY: browser-format
browser-format: browser-deps
	@$(BROWSER_RUN) npm exec -- prettier --write .

.PHONY: e2e-list e2e-durations
e2e-list: browser-deps
	@$(BROWSER_RUN) npm --silent run test:e2e -- --list --reporter=json > .cache/e2e-list.json

e2e-durations: job-bootstrap
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- e2e-durations $(E2E_DURATION_ARGS)
