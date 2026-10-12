# Publishing (docs/shipping.md). `make ship` is the only way agents publish:
# preflight gates at the exact commit, evidence, push, pull request. It runs the
# same judge as the agent hooks: built from origin/dev, or (labelled
# non-authoritative) from this checkout's committed HEAD while bootstrapping;
# never from uncommitted edits. SHIP_TITLE, SHIP_BODY, SHIP_FORCE and SHIP_BASE reach it
# through the environment only; they are never pasted into shell text.
# PR is the public Make argument; REVIEW_PR is the process environment value
# consumed by the harness. Neither is embedded in recipe shell text.
# PR= on the command line wins over a REVIEW_PR already in the environment;
# it reaches the harness through the exported variable, never shell text.
override REVIEW_PR := $(or $(PR),$(REVIEW_PR))
export DUEL_TASK DUEL_RUNTIME DUEL_MODEL DUEL_EFFORT DUEL_ROUNDS DUEL_CHECK DUEL_BUDGET_MINUTES
export SHIP_TITLE SHIP_BODY SHIP_FORCE SHIP_BASE SHIP_TIER SHIP_RUNTIME SHIP_VIDEO SHOWCASE_STORYBOARD SHOWCASE_OUT REVIEW_TIER REVIEW_RUNTIME REVIEW_TASK REVIEW_PR CALIBRATE_TIER CALIBRATE_RUNTIME CALIBRATE_CASES NIGHTLY_RESULTS
export MAKE BROWSER_IMAGE SHIP_TOOLS_IMAGE
.PHONY: duel duel-check ship ship-status review review-floor review-calibrate review-pr issue next nightly-triage video-probe ship-tools showcase showcase-check
SHIP_TOOLS_IMAGE := aoeworld/ship-tools:5.1.9
duel:
	@.agents/hooks/harness.sh exec duel

export HARNESS_DUEL_BASE HARNESS_DUEL_REVISION
duel-check:
	@docker run --rm --init --user $(UID):$(GID) -e CARGO_HOME=$(ROOT)/.cache/cargo -e GIT_INDEX_FILE -e AOE_SHIP_EVIDENCE -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION $(ROOT_MOUNTS) -w $(ROOT) $(TOOL_IMAGE) cargo run --locked -p aoe-harness -- duel-check
	@docker run --rm --init --network host --user $(UID):$(GID) --group-add $(shell stat -c %g /var/run/docker.sock) -e CARGO_HOME=$(ROOT)/.cache/cargo -e HARNESS_DUEL_BASE -e HARNESS_DUEL_REVISION -e AOE_ASSET_PACK $(ROOT_MOUNTS) -v /var/run/docker.sock:/var/run/docker.sock -w $(ROOT) $(ORCH_IMAGE) cargo run --locked -p aoe-harness -- duel-browser-check

ship:
	@.agents/hooks/harness.sh exec ship

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

# Duration and sound of SHIP_VIDEO (read from the environment), for make ship.
video-probe:
	@docker run --rm -v "$$SHIP_VIDEO":/video:ro "$$BROWSER_IMAGE" sh -c 'ffmpeg=$$(ls /ms-playwright/ffmpeg-*/ffmpeg-linux); $$ffmpeg -hide_banner -i /video 2>&1; true'

ship-tools:
	@docker build -q -f docker/ship-tools.Dockerfile -t "$$SHIP_TOOLS_IMAGE" . >/dev/null

# The showcase video of SHOWCASE_STORYBOARD (docs/showcase.md); Rust checks the
# voiced plan before invoking Make to build the pinned recording images.
showcase: showcase-check
	@.agents/hooks/harness.sh exec showcase

showcase-check:
	@.agents/hooks/harness.sh exec showcase-check
