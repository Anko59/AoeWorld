# Roadmap: harness and process

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| A better test run policy: pull requests that touch different parts of the code run different tests, so everything does not run all the time | 11-26 | IN FLIGHT `harness/ci-selection` | [CI selection](../ci-selection.md): the crate dependency graph decides the jobs, the unit scope and the static legs; `make ci-explain` and the `select` job summary say why | Ship it; then split the instruction kernels out of `perf-ci`, so a client change skips them, and let local `make ship` preflight follow the same rules |
| Every owner handoff proven by a real-match playtest, with logged errors | 10-26, 10-30 x2 | PARTIAL | #586 shows the real-match playtest in the PR (advisory); #830 quality metrics; 11-15 build shipped with new content never played end to end | Content e2e in the build handoff DROPPED 11-20 (owner: "Sounds overkill"); keep the real-match playtest and quality metrics |
| Simpler, faster tests; net-negative lines | 10-27, 11-13 x2 | PARTIAL | #845, #847, #799, #797 and the 10-27 wave (#582 to #604) | Keep deleting; no gate may block a justified behaviour change |
| Release evidence job: coverage ran 60+ minutes and failed 90 dev runs | 11-13 | PARTIAL | #808, #818, #841 | Issue #956: split the coverage job and move it to a nightly (harness session) |
| Update issue 748 body and #745 body from this roadmap | 11-16 | IN FLIGHT `roadmap/cleanup-11-21` | Issue 748 body is the 11-09 snapshot, #745 body is the 11-09 list | This cleanup rewrites the #748 body as a pointer to docs/roadmap.md; update the #745 body next |
| A make command for the CI report page: counts, times and reliability over time | 11-26 | IN FLIGHT `harness/ci-report` | Owner 11-26: "I want a make command to get the report page that you gave me earlier"; `make ci-report` ([CI report page](../ci-report.md)) | Merge it, then flip this row to DONE with its PR number |
| Replay recorder for live matches | 11-09 | PLANNED | See [scale](scale.md) | |
| The performance comment measures real play on real maps (arrival, movement, planning, fights, deaths, chokes, resources over time, unit management, early to late game), no more synthetic CI maps | 11-26 | IN FLIGHT `harness/game-score` | Steps 1 and 2: `make game-score` plays `natural` (10,000 ticks, early and middle windows) and `late` (the late-game recipe at 100 percent) on the real map, Rust AIs on every seat, through `aoe_server::HeadlessMatch`, reports arrival, planning, fights, deaths, deliveries over time, management work and cost per window and per game minute, and replays both at 1 and N workers ([game score](../scale/game-score.md)) | The choke battle and the bridge march with the choke-crossing counter; then the Callgrind child; then the CI job and the comment; then the gate |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| `make quality-metrics` with combat order fixed | 11-13, 11-14 | DONE | #830, #840 |
| Save format version named before decoding, layout changes need a bump | 11-14 | DONE | #839, #842, #843 |
| Order volume and command cost by issuer | 11-10 | DONE | #834 |
| Stored replay-hash pins removed | 11-13 | DONE | #799 |
| Machine-wide build pool, no heavy queue, sccache | 11-03 | DONE | #821, #670, #663 |
| Red-proof gate, heavy-slot queue | 10-20, 11-02 | DROPPED | Owner deleted `make red` 10-20; queue replaced by #670 |
