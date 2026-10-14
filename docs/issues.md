# Issues

Use GitHub issues as the shared list of work outside the current pull request.
When you notice out-of-scope work, file it and leave it for a follow-up:
**notice it, file it, don't fix it in this PR.**
The main session, testers and implementers may use these commands; reviewers'
findings are filed later by `make ship` (#174).

```sh
printf 'Flaky perf-smoke timeout\nlabels: priority:medium, area:harness\n\nThe timeout reproduces under load; investigate the worker queue.\n' |
  make issue
make next
```

The entire issue document is read from standard input; `make issue` takes no
Make variables. Line 1 is the title (up to 256 characters). An optional second
line has the form `labels: a, b`, followed by a blank line and the body. Bodies
are limited to 60,000 characters including the harness fingerprint marker.
Newlines and tabs are allowed; other control characters are refused. Optional labels are
limited to `priority:critical`, `priority:high`, `priority:medium`,
`priority:low`, `blocked`, and `area:<lowercase-dashed-name>`. The harness
adds `agent-found`.
Issue commands always target the GitHub repository named by the checkout's
`origin` remote.

Filing computes a fingerprint from the title, ignoring case, punctuation and
repeated whitespace. An open issue is a duplicate only when its title's
recomputed fingerprint matches, its body contains the matching marker, and it
has the `agent-found` label. In that case the harness comments on it instead
of opening another issue.

`make next` orders unblocked open issues by `priority:critical`, `priority:high`,
`priority:medium`, unlabelled, then `priority:low`; within each priority it
chooses the oldest issue first. An issue labelled `blocked` waits on its
blocking issue to be resolved, never on a person.

## Review follow-ups

After a **passing** review, `make ship` files what the review left
(`crates/harness/src/ship/issues/followups.rs`), through the same listing,
fingerprint and deduplication as `make issue`:

- one issue per confirmed or disputed **critical** or **major** finding, titled
  `Review finding at <file>:<line>: <claim>` and labelled `agent-found`,
  `review-follow-up` and `priority:critical` or `priority:high`;
- one **standing** issue, `Review follow-ups` (`priority:low`), for the **minor**
  and **nit** findings: one section per pull request, headed
  `## PR #<pr> — <url>`, with a `- [ ] <file>:<line> — <claim>` line per
  finding. The first such review opens the issue; later ones append their
  section, and a pull request shipped again replaces its own. A passing review
  with no remaining minor or nit findings removes that pull request's section
  and posts a comment stating that none remain at the current revision.
  Otherwise, the same list is posted as a comment on that pull request.

No issue is opened per pull request. Updates splice only the current pull
request's section into a fresh body, preserving every other byte, including
checked findings and owner edits, except when folding is needed. After the
splice, while there are more than 30 sections or the body would not fit within
60,000 characters including its fingerprint marker, the oldest sections fold
into one summary line counting the sections folded and the last PR folded.
Retained sections stay byte-identical. Before writing, the harness checks that
`updatedAt` still matches the read; a change restarts the splice and folding
from a fresh body, up to three attempts. If the standing issue stays busy or
cannot fit even after folding down to one section, it is left untouched and
the pull request comment is still posted. GitHub issue edits have no
compare-and-set, so an edit after the last timestamp check can still race the
write. The issue is found by its fingerprint
marker and the `agent-found` label, as duplicates are; `make ship` runs under
the person's `gh` account, so unlike nightly issues it is not authored by
`github-actions[bot]`.

Refuted findings are not leftovers. Reviewer text is untrusted: control
characters and `<` are neutralised, so it can never carry the fingerprint
marker, and every body is built in memory and sent on standard input. A
follow-up that cannot be filed is reported and never fails the ship: the review
already passed; file it with `make issue`.

## Nightly triage

The nightly workflow's last job, `triage` (`needs` every other job,
`if: always()`, `issues: write`, the workflow's `GITHUB_TOKEN`), runs
`make nightly-triage` (`crates/harness/src/ship/issues/triage.rs`). It reads
the workflow's `needs` context JSON from `NIGHTLY_RESULTS`: at most 64 KiB and
64 jobs, GitHub job ids, a `result` of `success`, `failure`, `cancelled` or
`skipped`, string `outputs`, and nothing else. For each job:

- `failure` opens `Nightly failure: <job>` (label `nightly-failure`) or comments
  on that open issue with the run link;
- `success` closes that issue with the run link;
- `cancelled` and `skipped` prove nothing and change nothing.

It touches only the issues it opened: authored by `github-actions[bot]`,
labelled `nightly-failure`, titled exactly `Nightly failure: <job>`, and whose
last fingerprint marker matches that title. An issue a person or agent opens
with the same title, label and marker is not commented on or closed. The
guarantee rests on the author: anything able to act as `github-actions[bot]`
in this repository (a workflow with `issues: write`, which only reaches `dev`
through review and CI) could still forge one. It
targets origin's repository, never `GITHUB_REPOSITORY`. Every job is tried
before a failed GitHub call fails the triage. The command belongs to CI and
the main session: the agent policy refuses `make nightly-triage` to every
subagent, and the harness refuses any other `AOE_AGENT_ROLE`. Run locally, it
acts as the person's account: the issues it opens are not the bot's, so CI
never comments on or closes them. Use it locally only to rehearse, and close
what it opened.

## Nightly metric regressions

The same `triage` job runs `make metric-regressions`
(`crates/harness/src/ship/issues/regressions.rs`) after the failure triage
(`if: always()`, a full-history checkout). It compares the static metrics
([shipping](shipping.md#metrics)) of dev's HEAD with dev as of
`window_hours` earlier (`git rev-list -1 --before`) and keeps ONE issue,
`Nightly: metric regressions`, using the thresholds in
`gates/metric-regressions.json`:

- the test count (all layers) fell by more than `max_test_drop`;
- `unsafe` lines rose by more than `max_unsafe_increase`;
- TODO/FIXME lines rose by more than `max_todo_increase`;
- third-party dependencies rose by more than `max_dependency_increase`;
- more than `max_files_over_limit` human-authored files exceed 500 lines
  (this one needs no earlier commit).

While any regression stands the issue is opened or its body is replaced with
the current list and the run link; when none remains it is closed. It follows
the ownership rules above: only the issue authored by `github-actions[bot]`,
labelled `nightly-failure`, titled exactly so and carrying the title's
fingerprint marker is edited or closed. The command belongs to CI and the main
session, like `make nightly-triage`.
