//! The decision as text an agent or a reviewer can read: one line per job.
use super::{decide::UnitScope, local::LocalDecision};
use std::collections::BTreeMap;

fn scope_line(parts: &[String], unit: &UnitScope) -> String {
    format!(
        "{}; static legs: {}; perf-smoke {}",
        unit.selection.line(),
        parts.join(", "),
        if unit.perf_smoke { "runs" } else { "skipped" }
    )
}

/// What `make ci-explain` prints: the jobs in name order, then the unit scope.
pub(crate) fn text(
    jobs: &BTreeMap<String, bool>,
    reasons: &BTreeMap<String, String>,
    parts: &[String],
    unit: &UnitScope,
) -> String {
    let mut out = String::new();
    for job in jobs.keys() {
        let reason = reasons.get(job).map_or("", String::as_str);
        out.push_str(&format!("{job}: {reason}\n"));
    }
    out.push_str(&format!("{}\n", scope_line(parts, unit)));
    out
}

/// What `make ci-explain` adds after the jobs: the checks `make ship` runs and CI does not.
pub(crate) fn local(decisions: &BTreeMap<String, LocalDecision>) -> String {
    decisions
        .iter()
        .map(|(name, decision)| format!("{name} (make ship, not a CI job): {}\n", decision.reason))
        .collect()
}

/// The job summary of the `select` job.
pub(crate) fn markdown(
    jobs: &BTreeMap<String, bool>,
    reasons: &BTreeMap<String, String>,
    parts: &[String],
    unit: &UnitScope,
) -> String {
    let mut out = String::from("## CI selection\n\n| job | runs | why |\n|---|---|---|\n");
    for (job, runs) in jobs {
        let reason = reasons.get(job).map_or("", String::as_str);
        out.push_str(&format!(
            "| `{job}` | {} | {reason} |\n",
            if *runs { "**yes**" } else { "no" }
        ));
    }
    out.push_str(&format!("\n{}\n", scope_line(parts, unit)));
    out
}
