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

`make next` first lists open pull requests into `dev` with auto-merge armed
that the PR updater could not merge with `dev` on their current head, as
`reship: conflicts with dev in <files>`, from the updater's conflict comment.
Then it lists those whose latest carry comment on their current head refuses
`harness/review`, with `reship: <reason>`. An older head's conflict or refusal,
or a later successful carry, does not appear. This command only reads GitHub.

It then orders unblocked open issues by `priority:critical`, `priority:high`,
`priority:medium`, unlabelled, then `priority:low`; within each priority it
chooses the oldest issue first. An issue labelled `blocked` waits on its
blocking issue to be resolved, never on a person.

## Review follow-ups and nightly triage

Ship records confirmed or disputed review findings by path in the workstream
issues configured in `gates/issues.json`: game findings under “Game findings”
in #575, and other findings in #576. Nightly job failures and metric regressions
go to #574. Stable per-PR and per-job sections deduplicate repeat reports,
preserve checked matching findings, and retain other issue text; configured
targets must be open. These paths never create, retitle, or close issues.
Updates check `updatedAt` and retry concurrent changes, though GitHub does not
provide an atomic compare-and-set.
