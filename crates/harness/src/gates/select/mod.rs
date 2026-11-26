//! Which CI jobs a change needs, derived from the crate dependency graph.
//!
//! A path inside a crate selects the jobs whose crates build on it; paths outside the
//! crates follow the explicit rules of `gates/ci-rules.json`; a path no rule covers
//! selects everything. See `docs/ci-selection.md`.
mod decide;
mod explain;
mod graph;
mod rules;
pub(crate) use decide::{Decision, UnitScope, decide, eligible};
pub(crate) use explain::{markdown, text};
pub(crate) use graph::Graph;
pub(crate) use rules::Rules;

/// The unit scope the select job handed to a leg of the `static` job (`AOE_UNIT_SCOPE`).
/// `AOE_TEST_ALL=1` forces the whole workspace; a scope that does not parse is an error,
/// never a silent fallback to less testing.
pub(crate) fn scope_from_env(
    get: impl Fn(&str) -> Option<String>,
) -> Result<Option<UnitScope>, Box<dyn std::error::Error>> {
    if get("AOE_TEST_ALL").is_some_and(|v| !v.is_empty() && v != "0") {
        return Ok(None);
    }
    get("AOE_UNIT_SCOPE")
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|e| format!("AOE_UNIT_SCOPE is not a unit scope: {e}").into())
        })
        .transpose()
}

#[cfg(test)]
mod tests;
