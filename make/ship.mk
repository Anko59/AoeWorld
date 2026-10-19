# Publishing (docs/shipping.md). `make ship` is the only way agents publish:
# preflight gates at the exact commit, evidence, push, pull request. It runs the
# same judge as the agent hooks: built from origin/dev, or (labelled
# non-authoritative) from this checkout's committed HEAD while bootstrapping;
# never from uncommitted edits. `ship`, `duel` and `red` share one machine-wide lock
# (SHIP_LOCK_WAIT_MINUTES bounds the wait). SHIP_TITLE, SHIP_BODY, SHIP_FORCE and SHIP_BASE reach it
# through the environment only; they are never pasted into shell text.
# PR is the public Make argument; REVIEW_PR is the process environment value
# consumed by the harness. Neither is embedded in recipe shell text.
# PR= on the command line wins over a REVIEW_PR already in the environment;
# it reaches the harness through the exported variable, never shell text.
override REVIEW_PR := $(or $(PR),$(REVIEW_PR))
# Video paths are data, including literal Make/shell metacharacters.
override SHIP_VIDEO := $(value SHIP_VIDEO)
export DUEL_TASK DUEL_RUNTIME DUEL_MODEL DUEL_EFFORT DUEL_ROUNDS DUEL_CHECK DUEL_BUDGET_MINUTES DUEL_QUALIFY DUEL_QUALIFY_MINUTES DUEL_QUALIFY_ATTEMPTS
export SHIP_TITLE SHIP_BODY SHIP_FORCE SHIP_BASE SHIP_TIER SHIP_RUNTIME SHIP_LOCK_WAIT_MINUTES SHIP_VIDEO SHOWCASE_STORYBOARD SHOWCASE_OUT REVIEW_TIER REVIEW_RUNTIME REVIEW_TASK REVIEW_PR CALIBRATE_TIER CALIBRATE_RUNTIME CALIBRATE_CASES NIGHTLY_RESULTS CARRY_PR UPDATE_MAX_PRS METRICS_REV
export COMPARE_ASSET_PACK COMPARE_BOOKMARKS COMPARE_VIEWS COMPARE_BEFORE COMPARE_AFTER COMPARE_BEFORE_PACKAGES COMPARE_AFTER_PACKAGES COMPARE_BASE COMPARE_OUT COMPARE_VIEW_TIMEOUT_SECONDS COMPARE_BUDGET_MINUTES
export MAKE BROWSER_IMAGE SHIP_TOOLS_IMAGE
export HARNESS_RED_ROOT HARNESS_RED_BASE HARNESS_RED_HEAD
.PHONY: red red-checkout red-checkout-steps
red:
	@.agents/hooks/harness.sh exec red

# Explicit branch runner for developing the command before it exists on dev.
red-checkout: slot-bootstrap
	@$(SLOT_RUN) red-checkout -- make --no-print-directory red-checkout-steps

red-checkout-steps:
	@$(DOCKER_RUN) cargo build --locked -p aoe-harness
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' red

.PHONY: load-status duel duel-check ship ship-status describe-check review review-floor level-floor level-stats review-calibrate review-pr issue next nightly-triage metric-regressions ledger usage carry-review update-prs metrics video-probe ship-tools showcase showcase-check showcase-session compare dev-server dev-server-status dev-server-stop
SHIP_TOOLS_IMAGE := aoeworld/ship-tools:5.1.9
duel:
	@.agents/hooks/harness.sh exec duel

export HARNESS_DUEL_BASE HARNESS_DUEL_REVISION

ship:
	@.agents/hooks/harness.sh exec ship

# Who holds the heavy slots and who queues for them (read-only; docs/gates.md).
load-status:
	@$(DOCKER_RUN) cargo run --locked -p aoe-harness -- load-status

ship-status:
	@.agents/hooks/harness.sh exec ship-status

# The tiered adversarial review of HEAD (docs/review.md): REVIEW_TIER (default:
# the floor), REVIEW_RUNTIME and REVIEW_TASK reach it through the environment.
review:
	@.agents/hooks/harness.sh exec review

review-floor:
	@.agents/hooks/harness.sh exec review-floor

# Run the review on the calibration seeds (docs/review.md): CALIBRATE_TIER
# (default low), CALIBRATE_RUNTIME and CALIBRATE_CASES (a sample size) reach it
# through the environment. A tool for tuning the reviewers, not a gate.
review-calibrate:
	@.agents/hooks/harness.sh exec review-calibrate

# Review a same-repository PR the harness did not open. REVIEW_PR is passed
# through the environment and never interpolated into shell text.
review-pr:
	@.agents/hooks/harness.sh exec review-pr

# File out-of-scope work; title, optional labels and body arrive on standard input.
issue:
	@.agents/hooks/harness.sh exec issue

# Print the next open, unblocked issue by priority and age.
next:
	@.agents/hooks/harness.sh exec next

# The nightly workflow's last job (docs/issues.md): NIGHTLY_RESULTS, its
# `needs` context JSON, reaches the harness through the environment only.
nightly-triage:
	@.agents/hooks/harness.sh exec nightly-triage

# The nightly workflow's metric comparison (docs/issues.md): one bot-owned issue.
metric-regressions:
	@.agents/hooks/harness.sh exec metric-regressions

# Which model did what, with measured results (docs/models.md); cached in .cache/ledger.
ledger:
	@.agents/hooks/harness.sh exec ledger

# What is left of each provider's allowance, then the last 7 days of the ledger.
usage:
	@.agents/hooks/harness.sh exec usage

# The post-push workflow (docs/post-push.md): CARRY_PR names the pull request,
# UPDATE_MAX_PRS bounds one update run; both reach the harness through the environment.
carry-review:
	@.agents/hooks/harness.sh exec carry-review

update-prs:
	@.agents/hooks/harness.sh exec update-prs

# The metrics snapshot of HEAD, or of METRICS_REV (docs/shipping.md); read-only.
metrics:
	@.agents/hooks/harness.sh exec metrics

# Duration and sound of SHIP_VIDEO (read from the environment), for make ship.
video-probe:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm $(call BIND_MOUNT,$(SHIP_VIDEO),/video,readonly) "$$BROWSER_IMAGE" sh -c 'ffmpeg=$$(ls /ms-playwright/ffmpeg-*/ffmpeg-linux); $$ffmpeg -hide_banner -i /video 2>&1; true'

ship-tools:
	@docker build -q -f docker/ship-tools.Dockerfile -t "$$SHIP_TOOLS_IMAGE" . >/dev/null

# The showcase video of SHOWCASE_STORYBOARD (docs/showcase.md); Rust checks the
# voiced plan before invoking Make to build the pinned recording images.
showcase: showcase-check
	@.agents/hooks/harness.sh exec showcase

showcase-check:
	@.agents/hooks/harness.sh exec showcase-check

# Every problem of the description (SHIP_BODY, SHIP_TITLE, SHIP_VIDEO), as
# `make ship` checks it up front; read-only, no lock.
describe-check:
	@.agents/hooks/harness.sh exec describe-check

# The lowest PR level the change allows (gates/levels.json), and the levels of
# the last merged PRs against the target distribution (uses gh, read-only).
level-floor:
	@.agents/hooks/harness.sh exec level-floor

level-stats:
	@.agents/hooks/harness.sh exec level-stats

# Convert a headless agent log (SESSION=<jsonl>, claude stream-json or codex
# exec --json) into the steps of an agent-session scene; read-only.
showcase-session:
	@.agents/hooks/harness.sh exec showcase-session

# Before and after screenshots of the bookmarked cameras of qa/bookmarks.json
# (docs/showcase.md, "Compare"). Builds and serves both sides on free ports,
# so it needs no running server; read-only towards the repository.
compare:
	@.agents/hooks/harness.sh exec compare

# The shared dev server (docs/dev-server.md): built from origin/dev in a
# harness-owned checkout, on a fixed port, with the preset maps prepared once.
dev-server:
	@.agents/hooks/harness.sh exec dev-server

dev-server-status:
	@.agents/hooks/harness.sh exec dev-server-status

dev-server-stop:
	@.agents/hooks/harness.sh exec dev-server-stop

# Injected readiness hooks need Chromium, but no game server or assets.
.PHONY: test-showcase-browser test-showcase-browser-steps
test-showcase-browser: browser-deps slot-bootstrap
	@$(SLOT_RUN) test-e2e -- make --no-print-directory test-showcase-browser-steps

test-showcase-browser-steps:
	@$(BROWSER_RUN) npx playwright test tests/showcase/ --project=webgpu --output=../.cache/showcase-browser-tests

.PHONY: test-showcase test-showcase-steps
test-showcase: slot-bootstrap
	@$(SLOT_RUN) test-harness -- make --no-print-directory test-showcase-steps

test-showcase-steps:
	@$(DOCKER_RUN) cargo test --locked -p aoe-harness showcase

# Read-only report using this checkout before the command reaches the dev judge.
export TIMELINE_DAYS
.PHONY: timeline
timeline: slot-bootstrap
	@'$(HARNESS_TARGET_CACHE)/debug/aoe-harness' timeline
