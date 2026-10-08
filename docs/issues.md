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
- one checklist issue per pull request, `Review follow-ups for #<pr>`
  (`priority:low`), with a `- [ ] <file>:<line> — <claim>` line per confirmed or
  disputed **minor** or **nit** finding.

Refuted findings are not leftovers. Reviewer text is untrusted: control
characters and `<` are neutralised, so it can never carry the fingerprint
marker, and every body is built in memory, clipped so that it stays within
60,000 characters with the marker appended, and sent on standard input. A
second ship of the same pull request comments on the open issues instead of
opening new ones; the checklist's comment repeats the new review's list, so a
finding new in that round is kept. A follow-up that cannot be filed is reported and never fails
the ship: the review already passed; file it with `make issue`.

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
