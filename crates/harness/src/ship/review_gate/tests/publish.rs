use super::super::{Result, publish_calls_onto};
use super::Report;

/// The `gh` calls that publish a passing review into dev: the
/// `harness/review` success status on the reviewed commit, then GitHub
/// auto-merge (squash).
pub(in crate::ship) fn publish_calls(
    repository: &str,
    url: &str,
    report: &Report,
) -> Result<Vec<Vec<String>>> {
    publish_calls_onto(repository, url, report, crate::review::base::DEV)
}
