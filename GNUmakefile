# GNU make reads this file before Makefile. Linux uses Makefile unchanged; a Mac
# has no Linux kernel for the harness and its containers, so every goal runs
# inside the Colima VM, where the checkout is mounted at the same path
# (docs/local-development.md). With AOE_NATIVE_SHIP=1 a ship goal runs on the Mac.
ifeq ($(shell uname -s),Darwin)
NATIVE_GOALS := $(if $(filter 1,$(AOE_NATIVE_SHIP)),$(shell AOE_NATIVE_SHIP=1 .agents/hooks/mac-judge.sh goals))
ifeq ($(if $(filter-out $(NATIVE_GOALS),$(or $(MAKECMDGOALS),help)),vm,mac),mac)
include make/ship.mk
else
COLIMA_GOALS := $(or $(MAKECMDGOALS),help)
COLIMA_VARS := $(strip $(foreach name,$(.VARIABLES),$(if $(filter command,$(origin $(name))),$(name))))
# Ship asks Make which goals exist (`make -rRqp`): the VM's are declared here too.
COLIMA_KNOWN := $(filter-out $(COLIMA_GOALS),$(sort $(shell sed -n 's/^\([a-z][a-z0-9-]*\):.*/\1/p' Makefile make/*.mk)))
.PHONY: colima-delegate $(COLIMA_GOALS) $(COLIMA_KNOWN)
$(COLIMA_GOALS) $(COLIMA_KNOWN): colima-delegate
	@:
colima-delegate:
	@AOE_COLIMA_MAKE_VARS='$(COLIMA_VARS)' .agents/hooks/colima.sh make --no-print-directory $(COLIMA_GOALS)
endif
else
include Makefile
endif
