# Publishing (docs/shipping.md). `make ship` is the only way agents publish:
# preflight gates at the exact commit, evidence, push, pull request. It runs the
# same judge as the agent hooks: built from origin/dev, or (labelled
# non-authoritative) from this checkout's committed HEAD while bootstrapping;
# never from uncommitted edits. SHIP_TITLE, SHIP_BODY and SHIP_FORCE reach it
# through the environment only; they are never pasted into shell text.
export SHIP_TITLE SHIP_BODY SHIP_FORCE
.PHONY: ship ship-status
ship:
	@.agents/hooks/harness.sh exec ship

ship-status:
	@.agents/hooks/harness.sh exec ship-status
