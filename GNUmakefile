# GNU make reads this file before Makefile. Linux uses Makefile unchanged; a Mac
# has no Linux kernel for the harness and its containers, so every goal runs
# inside the Colima VM, where the checkout is mounted at the same path
# (docs/local-development.md).
ifeq ($(shell uname -s),Darwin)
COLIMA_GOALS := $(or $(MAKECMDGOALS),help)
COLIMA_VARS := $(strip $(foreach name,$(.VARIABLES),$(if $(filter command,$(origin $(name))),$(name))))
.PHONY: colima-delegate $(COLIMA_GOALS)
$(COLIMA_GOALS): colima-delegate
	@:
colima-delegate:
	@AOE_COLIMA_MAKE_VARS='$(COLIMA_VARS)' .agents/hooks/colima.sh make --no-print-directory $(COLIMA_GOALS)
else
include Makefile
endif
