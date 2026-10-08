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

Review follow-ups and nightly issue triage are planned in a follow-up issue;
they are not part of these commands yet.
