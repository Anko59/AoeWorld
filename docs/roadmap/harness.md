# Roadmap: harness and process

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| A better test run policy: pull requests that touch different parts of the code run different tests, so everything does not run all the time | 11-26 | PARTIAL | [CI selection](../ci-selection.md): the crate dependency graph decides the jobs, the unit scope and the static legs; `make ci-explain` and the `select` job summary say why. #1121 landed PR-specific checks. #1126 made PR Docker layers read-only, #1132 added per-leg Rust dependency caches, and #1133 moved the performance comment into target-performance and limited setup to what each job uses. | Split instruction kernels from perf-ci for client-only changes and align local ship selection. |
| Every owner handoff proven by a real-match playtest, with logged errors | 10-26, 10-30 x2 | PARTIAL | #586 shows the real-match playtest in the PR (advisory); #830 quality metrics; 11-15 build shipped with new content never played end to end | Content e2e in the build handoff DROPPED 11-20 (owner: "Sounds overkill"); keep the real-match playtest and quality metrics |
| Simpler, faster tests; net-negative lines | 10-27, 11-13 x2 | PARTIAL | #845, #847, #799, #797 and the 10-27 wave (#582 to #604). #1119 removed native assertions on host timing and load. #1131, #1127 and #1134 fixed flaky browser recovery, memory, selection, far-build and bulk-Market tests. | Keep deleting; no gate may block a justified behaviour change |
| Release evidence job: coverage ran 60+ minutes and failed 90 dev runs | 11-13 | PARTIAL | #808, #818, #841 | Issue #956: split the coverage job and move it to a nightly (harness session) |
| Update issue 748 body and #745 body from this roadmap | 11-16 | PARTIAL | #748 now points to the roadmap. #745 still has its 11-09 list. | Update #745. |
| The performance comment measures real play on real maps (arrival, movement, planning, fights, deaths, chokes, resources over time, unit management, early to late game), no more synthetic CI maps | 11-26 | PARTIAL | #1128 landed real-map arrival, planning, combat, deliveries and management windows for natural and late matches, deterministic at one and four workers. #1142 added staged marches and battles across an Anatolia ford with crossing and combat counters. #1135 added the local ship comparison against the merge base, published as an informational PR comment. | Callgrind integration and CI enforcement remain open. #1124 left the map-cache warm job manual after HTTP 403 from HydroSHEDS. |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Replay recorder for live matches | 11-09 | DONE | #802 records applied commands in a live saved match; #984 includes human camera input and #988 records tick rate. |
| A make command for the CI report page: counts, times and reliability over time | 11-26 | DONE | #1115 merged `make ci-report`, producing an HTML report and JSON with job times, gate standing and reliability. |
| `make quality-metrics` with combat order fixed | 11-13, 11-14 | DONE | #830, #840 |
| Save format version named before decoding, layout changes need a bump | 11-14 | DONE | #839, #842, #843 |
| Order volume and command cost by issuer | 11-10 | DONE | #834 |
| Stored replay-hash pins removed | 11-13 | DONE | #799 |
| Machine-wide build pool, no heavy queue, sccache | 11-03 | DONE | #821, #670, #663 |
| Red-proof gate, heavy-slot queue | 10-20, 11-02 | DROPPED | Owner deleted `make red` 10-20; queue replaced by #670 |
