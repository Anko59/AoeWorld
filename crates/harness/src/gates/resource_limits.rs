//! Generated resource policy text for docs/gates.md.
pub(super) fn paragraph() -> String {
    [
        "**Resources and long jobs.** There is no ticket queue or CPU quota. Every build container takes one of a few build slots; long jobs also hold one of a small fixed number of long-job slots.",
        "Every build/test container uses the shared `gates/container.args` policy: 4 GiB memory including swap, relative CPU shares, and four Cargo build jobs. The harness's own Docker commands use the same caps.",
        "Before starting a container, the Rust launcher refuses immediately if host MemAvailable is below 3 GiB; the cap bounds one container, while admission checks the current host headroom. Concurrent admissions are observations, not memory reservations.",
        "A kernel OOM kill is reported as exceeding the container memory cap. Final cgroup memory.peak readings accompany Make container output.",
        "Only gates marked `long` (benchmarks, full E2E, coverage, and aggregates that run them) take one of N nonblocking kernel-lock slots, the files `<git common dir>/aoe-ship/long-0.lock` … `long-{N-1}.lock`; N is 3 unless `AOE_LONG_SLOTS` (1..=8) says otherwise. Focused `test-e2e` with nonempty `AOE_E2E_SPECS` is short; WASM size analysis is short too.",
        "A long job takes the first free slot; only when every slot is held does it refuse with exit 75 and `busy: slot 0: <holder command, branch, worktree, started>; slot 1: …, retry later`, listing every holder; it never waits. Make itself reports recipe failures as exit 2. Short jobs never take this lock.",
        "Every `docker run` that Make starts through the launcher holds one build slot, the files `<git common dir>/aoe-ship/cpu-0.lock` … `cpu-{N-1}.lock`; N is 4 unless `AOE_CPU_SLOTS` (1..=16) says otherwise, about 16 compilers on the 12-core host. One worktree may hold 2 build slots, and one coordinator with all its agents 3 (`AOE_COORDINATOR`, set by agent launchers). A build container waits up to 90 s for a slot, then refuses with exit 75 naming every holder; it never queues longer. A container inside a long job, or inside an admitted container (`AOE_CPU_TOKEN`), uses its parent's slot. Each admission, refusal and finish appends one JSON line to `<git common dir>/aoe-ship/ledger.jsonl`.",
        "Exec children inherit the lock descriptor, so killing the parent does not free it while those children run. Nested targets carry `AOE_LONG_JOB`, `AOE_LONG_DIR` and `AOE_LONG_SLOT` (the held slot), accepting ownership only while that slot's kernel lock and proof are live. Snapshots use the source repository's lock directory.",
        "The host job wrapper takes the lock before running long-target prerequisites. The independent std-only resource launcher bootstraps in a small 256 MiB container; workspace compilation is admitted and capped, without a machine-wide bootstrap lock.",
        "CI uses the same caps and immediate admission; each job has its own runner. `make load-status` shows MemAvailable, every long-job and build slot with its holder, and the latest ledger lines. No wait-time environment variables remain.",
        "",
        "",
    ].join("\n")
}
