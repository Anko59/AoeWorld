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
  assets-dat-table

# Formats browser tooling with the pinned formatter.
.PHONY: browser-format
browser-format: browser-deps
	@$(BROWSER_RUN) npm exec -- prettier --write .
