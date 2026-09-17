# Release flow

The intended flow is feature PRs to `dev`, verified artifact promotion to
`main`, and local promotion/rollback rehearsal. The local repository is in
bootstrap; protected branch settings, artifact signing, and release rehearsals
must be established before claiming release readiness. A bootstrap commit on
`main` is the documented one-time exception to normal PR rules.
