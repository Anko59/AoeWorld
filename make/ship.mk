# Publishing (docs/shipping.md). `make ship` is the only way agents publish:
# preflight gates at the exact commit, evidence, push, pull request. It runs the
# same judge as the agent hooks: built from origin/dev, or (labelled
# non-authoritative) from this checkout's committed HEAD while bootstrapping;
# never from uncommitted edits. SHIP_TITLE, SHIP_BODY and SHIP_FORCE reach it
# through the environment only; they are never pasted into shell text.
# PR is the public Make argument; REVIEW_PR is the process environment value
# consumed by the harness. Neither is embedded in recipe shell text.
# PR= on the command line wins over a REVIEW_PR already in the environment;
# it reaches the harness through the exported variable, never shell text.
override REVIEW_PR := $(or $(PR),$(REVIEW_PR))
export SHIP_TITLE SHIP_BODY SHIP_FORCE SHIP_TIER SHIP_RUNTIME SHIP_VIDEO SHOWCASE_STORYBOARD SHOWCASE_OUT REVIEW_TIER REVIEW_RUNTIME REVIEW_TASK REVIEW_PR NIGHTLY_RESULTS
export MAKE BROWSER_IMAGE SHIP_TOOLS_IMAGE
.PHONY: ship ship-status review review-floor review-pr issue next nightly-triage video-probe ship-tools showcase showcase-check
SHIP_TOOLS_IMAGE := aoeworld/ship-tools:5.1.9
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
