//! Per-command rules the Bash judge applies after flattening a line.
pub(crate) mod dispatch;
pub(crate) mod git;
pub(crate) mod github;
mod network;
pub(crate) mod tools;
pub(crate) mod writers;

/// How a change ships; every Git and GitHub denial ends with it.
pub(crate) const SHIP: &str = "commit on a feature branch (the pre-commit hook runs `make pre-commit`), push that branch (the pre-push hook runs `make preflight`) and open a pull request against `dev`; a person merges";
