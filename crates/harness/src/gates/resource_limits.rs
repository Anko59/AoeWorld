//! Generated resource policy text for docs/gates.md.
pub(super) fn paragraph() -> String {
    [
        "**Resources and long jobs.** Short gates start immediately across worktrees; there is no ticket queue, load wait, slot count or CPU quota.",
        "Every build/test container uses the shared `gates/container.args` policy: 4 GiB memory including swap, relative CPU shares, and six Cargo build jobs. The harness's own Docker commands use the same caps.",
        "Before starting a container, the Rust launcher refuses immediately if host MemAvailable is below 3 GiB; the cap bounds one container, while admission checks the current host headroom. Concurrent admissions are observations, not memory reservations.",
        "A kernel OOM kill is reported as exceeding the container memory cap. Final cgroup memory.peak readings accompany Make container output.",
        "Only gates marked `long` (benchmarks, full E2E, coverage, and aggregates that run them) take the nonblocking kernel lock at `<git common dir>/aoe-ship/long.lock`. Focused `test-e2e` with nonempty `AOE_E2E_SPECS` is short; WASM size analysis is short too.",
        "A second long job refuses with exit 75 and `busy: <holder command, branch, worktree, started>, retry later`; it never waits. Make itself reports recipe failures as exit 2. Short jobs never take this lock.",
        "Exec children inherit the lock descriptor, so killing the parent does not free it while those children run. Nested targets carry `AOE_LONG_JOB` and `AOE_LONG_DIR`, accepting ownership only while the kernel lock and its proof are live. Snapshots use the source repository's lock directory.",
        "The host job wrapper takes the lock before running long-target prerequisites. The independent std-only resource launcher bootstraps in a small 256 MiB container; workspace compilation is admitted and capped, without a machine-wide bootstrap lock.",
        "CI uses the same caps and immediate admission; each job has its own runner. `make load-status` shows MemAvailable and the current long-job holder. No wait-time environment variables remain.",
        "`make resource-measure RESOURCE_MEASURE_ARGS='--output .cache/measurement --job /absolute/worktree=lint'` records actual Make duration, final container peaks, once-per-second MemAvailable, uptime/free output and other running containers. Repeat `--job` to measure concurrent jobs; warm each worktree before comparing times.",
        "",
        "",
    ].join("\n")
}
