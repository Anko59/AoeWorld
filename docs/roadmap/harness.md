# Roadmap: harness and process

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Every owner handoff proven by a real-match playtest, with logged errors | 10-26, 10-30 x2 | PARTIAL | #586 shows the real-match playtest in the PR (advisory); #830 quality metrics; 11-15 build shipped with new content never played end to end | Content e2e in the build handoff DROPPED 11-20 (owner: "Sounds overkill"); keep the real-match playtest and quality metrics |
| Simpler, faster tests; net-negative lines | 10-27, 11-13 x2 | PARTIAL | #845, #847, #799, #797 and the 10-27 wave (#582 to #604) | Keep deleting; no gate may block a justified behaviour change |
| Release evidence job: coverage ran 60+ minutes and failed 90 dev runs | 11-13 | PARTIAL | #808, #818, #841 | Issue #956: split the coverage job and move it to a nightly (harness session) |
| Update issue 748 body and #745 body from this roadmap | 11-16 | IN FLIGHT `roadmap/cleanup-11-21` | Issue 748 body is the 11-09 snapshot, #745 body is the 11-09 list | This cleanup rewrites the #748 body as a pointer to docs/roadmap.md; update the #745 body next |
| Replay recorder for live matches | 11-09 | PLANNED | See [scale](scale.md) | |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| `make quality-metrics` with combat order fixed | 11-13, 11-14 | DONE | #830, #840 |
| Save format version named before decoding, layout changes need a bump | 11-14 | DONE | #839, #842, #843 |
| Order volume and command cost by issuer | 11-10 | DONE | #834 |
| Stored replay-hash pins removed | 11-13 | DONE | #799 |
| Machine-wide build pool, no heavy queue, sccache | 11-03 | DONE | #821, #670, #663 |
| Red-proof gate, heavy-slot queue | 10-20, 11-02 | DROPPED | Owner deleted `make red` 10-20; queue replaced by #670 |
