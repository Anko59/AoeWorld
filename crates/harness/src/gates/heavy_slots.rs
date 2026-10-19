//! The generated "Heavy slots" paragraph of docs/gates.md, one sentence per source line so two
//! branches that each change a different sentence do not conflict.

const SENTENCES: &[&str] = &[
    "**Heavy slots.** A gate marked `heavy` compiles or runs the workspace's tests, browsers or benchmarks.",
    "It takes one machine-wide slot for its whole run (all worktrees of the repository share `<git common dir>/aoe-ship/heavy-slots/`): two at a time by default (`gates/heavy.json`; `AOE_HEAVY_SLOTS=1..8` overrides), the rest queue first come, first served.",
    "Each slot gives its containers `cores / slots` CPUs (cores counted from the CPU affinity list, so 12 cores and 2 slots give 6 CPUs each): the slot's holder computes that figure and exports it as `AOE_SLOT_CPUS` beside `AOE_HEAVY_SLOT` (`aoe-harness slot-cpus` prints it), so Make only expands a variable a holder set; a container started under a slot takes `--cpus` and cargo's job count from it, including the browser and chromedriver containers the harness starts itself, and with no slot there is no cap and never an empty flag.",
    "In CI, where each job has its runner to itself, a slot exports no cap.",
    "Every heavy target takes its slot before its first container (single-command gates such as `fuzz-smoke`, `perf-smoke` and the release `map-*` targets run their `-steps` under `aoe-harness slot` too); the wrapper container itself only waits and orchestrates, so it is not capped.",
    "The deadline counts from when the gate starts running, never from the wait; waiting prints `heavy slot: waiting for <command> in <worktree> (position N)` and a line every 5 minutes, and gives up after `AOE_HEAVY_WAIT_MINUTES` (default 90) with an error naming the holders.",
    "Before starting, each heavy gate also waits up to 30 minutes while the 1-minute load exceeds the core count, printing every 5 minutes; the deadline starts after this wait too.",
    "A local `ship` or `preflight` gate (never in CI) that exceeds its deadline while the 1-minute load is above the core count waits for the load to fall, as above, and runs once more with the same budget; its evidence records both attempts, and a second timeout fails.",
    "SIGINT/SIGTERM cancels a load wait and releases its ticket.",
    "Load is checked only at admission: same-process stages and child processes inherit the completed check; a later rise never pauses or aborts running gates.",
    "Heavy gates use the registry's `heavy` flag; direct `test-harness` also takes a slot because it compiles heavily.",
    "A harness command takes the slot of the gate whose make target has its name.",
    "The `slot` command that make wrappers use takes a slot whatever the registry says, so a target that compiles without being a gate (`build`, `duel-check`) queues too.",
    "A holder passes its slot to its children in `AOE_HEAVY_SLOT`, and the shared directory in `AOE_HEAVY_DIR` (checked against its live ticket, so a hand-set value is ignored), so `preflight` running `test-unit` takes one slot, not two.",
    "Only a validated inherited slot skips the load wait; a child that acquires its own slot checks load.",
    "Supervised children receive the held slot and directory.",
    "Integration tests inherit their parent's slot or hold an admitted fixture ticket in a private queue.",
    "Every container a recipe starts forwards these variables (`HEAVY_ENV` in the Makefile, enforced by an audit test).",
    "A heavy gate whose recipe is not one harness command (`build-wasm`, `coverage`, `test-e2e`, the perf targets, and direct `test-harness`/`test-unit`/`test-wasm`) runs its `<target>-steps` in the orchestrator container under `aoe-harness slot`, so the slot is taken before its first step.",
    "Its command-only bootstrap is static work before admission, serialized across worktrees with flock and hard-limited to one CPU, one Cargo job and 2 GiB including swap; it builds no tests.",
    "These limits keep bootstrapping bounded even with cold caches, while the gate compilation remains slotted.",
    "A snapshot checkout queues in the real repository's directory, and a command outside every checkout has no heavy gate.",
    "Cheap static gates never queue.",
    "A dead holder frees its slot (an OS file lock, no PID files).",
    "`make load-status` (and `make ship-status`) list the holders and the queue.",
];

/// The paragraph with its closing blank line, as it appears in docs/gates.md.
pub(super) fn paragraph() -> String {
    format!("{}\n\n", SENTENCES.join(" "))
}
