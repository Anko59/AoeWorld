# Same army builder as perf-groups; instrument only movement at tick 201.
.PHONY: perf-groups-instructions perf-groups-instructions-steps
perf-groups-instructions: analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-groups-instructions -- make --no-print-directory perf-groups-instructions-steps

perf-groups-instructions-steps:
	@mkdir -p reports/perf
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) -e $(call MOUNT_QUOTE,CARGO_HOME=$(HARNESS_CARGO_CACHE)) -e GUNGRAUN_ALLOW_ASLR=yes -e CARGO_PROFILE_BENCH_DEBUG=1 $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) cargo bench --locked -p aoe-simulation --bench group_instructions -- --output-format=json --save-summary=json > reports/perf/group-instructions.ndjson

GROUP_CALLGRIND ?= target/gungraun/aoe-simulation/group_instructions/simulation/movement/callgrind.movement.out
GROUP_CALLGRIND_REPORT ?= reports/perf/group-callgrind-inclusive.txt
GROUP_CALLGRIND_INCLUSIVE ?= yes
.PHONY: perf-groups-annotate
perf-groups-annotate:
	@docker run $(HEAVY_ENV) $(SLOT_CAP) --rm --init --user $(UID):$(GID) $(ROOT_MOUNTS) -w $(ROOT) $(ANALYSIS_IMAGE) callgrind_annotate --show=Ir --inclusive=$(GROUP_CALLGRIND_INCLUSIVE) --threshold=100 --auto=no $(GROUP_CALLGRIND) > $(GROUP_CALLGRIND_REPORT)

# Keep the matched timing and instruction measurements in one admitted slot.
.PHONY: perf-groups-profile perf-groups-profile-steps
perf-groups-profile: analysis-tools slot-bootstrap
	@$(SLOT_RUN) perf-groups-profile -- make --no-print-directory perf-groups-profile-steps

perf-groups-profile-steps:
	@make --no-print-directory perf-groups-steps
	@make --no-print-directory perf-groups-instructions-steps
