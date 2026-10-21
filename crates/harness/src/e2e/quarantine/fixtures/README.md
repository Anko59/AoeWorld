# Playwright regression evidence

`ci-532-shard-3.json` is trimmed from PR #532's validation run 39540701382,
`e2e-3-diagnostics/reports/e2e/playwright-shard-3-of-3.json`. It retains all
59 selected IDs, test locations (including original columns), titles, projects,
final statuses, durations, error messages/stacks, assertion/hook step evidence,
and original counts.
Attachments, stdout/stderr, source snippets, passing step trees, unrelated
configuration and result timing fields are omitted. The file is generated JSON.

The original shard contains 33 expected, 12 unexpected, 14 skipped and zero
flaky results. Nine unexpected cases have actual status `timedOut`; three have
actual status `failed`. Some failed cases also mention deadlines in their
messages. Preserve the actual status rather than inferring it from that text.
All selected cases have results and the global errors array is empty.

The assertion, setup and worker fixtures are isolated real Playwright reports
used to pin recovery eligibility independently of failure diagnostics.
