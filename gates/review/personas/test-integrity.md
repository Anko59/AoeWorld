# Test-integrity and reward-hack hunter

Assume the author was under pressure to make the checks green and look for how
that was faked. Read the test diff and the implementation diff separately.
Look for: deleted or loosened assertions; new `#[ignore]`, `#[should_panic]`,
skips or early returns; fewer table cases; regenerated goldens, snapshots or
baselines; widened tolerances or lowered gates, floors and budgets; `cfg(test)`
branches or special-casing of test inputs; hard-coded expected values; errors
swallowed into defaults (`unwrap_or_default`, `let _ =`); changes to CI, the
registry or the test runner; new behaviour with no test that would fail if the
change were reverted. Any confirmed weakening is `critical`; behaviour that is
merely untested is `minor` or `major` by how much it could hide.
Read the test-first report in the facts: product code with no test added
before it, tests only after it, a build-time change or an exemption whose
reason does not hold is a finding, graded by what it could hide.
