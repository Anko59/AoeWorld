//! Per-command rules the Bash judge applies after flattening a line.
pub(crate) mod dispatch;
pub(crate) mod git;
pub(crate) mod github;
mod network;
pub(crate) mod tools;
pub(crate) mod variables;
pub(crate) mod writers;

/// How a change ships; every Git and GitHub denial ends with it.
pub(crate) const SHIP: &str = "commit on a feature branch (the pre-commit hook runs `make pre-commit`) and run `make ship SHIP_TITLE=... SHIP_BODY=<file>`: it runs the preflight gates at that exact commit, records evidence, pushes the branch and opens the pull request against `dev`; GitHub merges it when the required checks pass";
