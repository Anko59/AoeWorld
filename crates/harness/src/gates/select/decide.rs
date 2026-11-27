//! The decision: which jobs, which unit tests and which static checks a change needs.
use super::{
    graph::Graph,
    rules::{Class, HARNESS, Rules},
};
use crate::{
    gates::registry::{Cadence, Registry},
    native::affected::Selection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The `test-unit` scope of the unit leg, and whether it also runs the `perf-smoke` gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitScope {
    pub(crate) selection: Selection,
    pub(crate) perf_smoke: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Decision {
    pub(crate) jobs: BTreeMap<String, bool>,
    /// One sentence per job: "runs because ..." or "skipped because ...".
    pub(crate) reasons: BTreeMap<String, String>,
    /// The legs of the `static` job: `light` or `checks`, then `unit` when tests run.
    pub(crate) static_parts: Vec<String>,
    pub(crate) unit: UnitScope,
}

/// The jobs that have at least one gate the cadence runs.
pub(crate) fn eligible(registry: &Registry, cadence: Cadence) -> BTreeSet<String> {
    registry
        .jobs
        .iter()
        .filter(|(_, members)| {
            members.iter().any(|id| {
                registry
                    .gates
                    .iter()
                    .any(|g| &g.id == id && g.cadences.contains(&cadence))
            })
        })
        .map(|(job, _)| job.clone())
        .collect()
}

/// What the changed paths told, with the first path that told it.
#[derive(Default)]
struct Evidence {
    everything: Option<String>,
    /// Crates with a changed source file.
    code: BTreeMap<String, String>,
    /// Crates whose tests may read a changed file.
    reads: BTreeMap<String, String>,
    /// Jobs a path rule selects.
    jobs: BTreeMap<String, String>,
    checks: Option<String>,
    rust: bool,
}

fn gather(rules: &Rules, graph: &Graph, paths: &[String]) -> Evidence {
    let mut found = Evidence::default();
    if paths.is_empty() {
        found.everything =
            Some("no changed path: the base is unavailable or the diff is empty".to_owned());
    }
    for path in paths {
        match rules.classify(path, |p| graph.owner(p)) {
            Class::Everything(rule) => {
                found.everything.get_or_insert(format!("{path} ({rule})"));
            }
            Class::Unknown => {
                found
                    .everything
                    .get_or_insert(format!("{path} (no ci rule covers this path)"));
            }
            Class::Checks => {
                found.checks.get_or_insert(path.clone());
            }
            Class::Static => {}
            Class::Harness => {
                found
                    .reads
                    .entry(HARNESS.to_owned())
                    .or_insert(path.clone());
            }
            Class::Reads(jobs) => {
                for name in graph.mentioning(path) {
                    found.reads.entry(name).or_insert(path.clone());
                }
                for job in jobs {
                    found.jobs.entry(job).or_insert(path.clone());
                }
            }
            Class::Crate { name, jobs } => {
                found.rust |= graph.is_workspace(&name);
                found.code.entry(name).or_insert(path.clone());
                for job in jobs {
                    found.jobs.entry(job).or_insert(path.clone());
                }
            }
        }
    }
    found
}

/// Why `job` runs because of a changed crate, if one is in the closure of its crates.
fn graph_reason(rules: &Rules, graph: &Graph, job: &str, found: &Evidence) -> Option<String> {
    let rule = rules.jobs.get(job)?;
    for subject in &rule.crates {
        let closure = graph.closure(subject, rule.tests);
        if let Some((name, path)) = found.code.iter().find(|(c, _)| closure.contains(*c)) {
            return Some(if name == subject {
                format!("{name} changed ({path})")
            } else {
                format!("{subject} builds on {name}, which changed ({path})")
            });
        }
    }
    None
}

fn job_decision(rules: &Rules, graph: &Graph, job: &str, found: &Evidence) -> (bool, String) {
    if let Some(reason) = &found.everything {
        return (
            true,
            format!("runs because everything is selected: {reason}"),
        );
    }
    if !rules.jobs.contains_key(job) {
        return (true, "runs because no ci rule narrows it".to_owned());
    }
    if let Some(path) = found.jobs.get(job) {
        return (
            true,
            format!("runs because {path} is one of its inputs or plumbing"),
        );
    }
    if let Some(reason) = graph_reason(rules, graph, job, found) {
        return (true, format!("runs because {reason}"));
    }
    let crates = rules.jobs[job].crates.join(", ");
    (
        false,
        format!(
            "skipped because no changed crate is in the build closure of {crates} and no changed path is its input"
        ),
    )
}

/// Decide from the changed `paths`. `eligible` are the jobs the run's cadence can run.
pub(crate) fn decide(
    rules: &Rules,
    graph: &Graph,
    registry: &Registry,
    eligible: &BTreeSet<String>,
    paths: &[String],
) -> Decision {
    let found = gather(rules, graph, paths);
    let total = graph.workspace().len();
    let (unit, affected) = match &found.everything {
        Some(reason) => (
            UnitScope {
                selection: Selection::All {
                    total,
                    reason: reason.clone(),
                },
                perf_smoke: true,
            },
            total,
        ),
        None => {
            let changed: BTreeSet<String> = found
                .code
                .keys()
                .chain(found.reads.keys())
                .filter(|name| graph.is_workspace(name))
                .cloned()
                .collect();
            let affected = graph.tested_with_dependents(&changed);
            let perf_smoke = found.jobs.contains_key("target-performance")
                || rules.perf_smoke_crates.iter().any(|c| affected.contains(c));
            let count = affected.len();
            (
                UnitScope {
                    selection: Selection::Crates {
                        dependents: affected.difference(&changed).cloned().collect(),
                        changed: changed.into_iter().collect(),
                        total,
                    },
                    perf_smoke,
                },
                count,
            )
        }
    };
    let mut jobs = BTreeMap::new();
    let mut reasons = BTreeMap::new();
    for job in registry.jobs.keys() {
        let (runs, reason) = if job == "static" {
            (true, "runs on every pull request".to_owned())
        } else if !eligible.contains(job) {
            (
                false,
                "skipped because it runs after merge only (pushes to dev and the nightly run it)"
                    .to_owned(),
            )
        } else {
            job_decision(rules, graph, job, &found)
        };
        jobs.insert(job.clone(), runs);
        reasons.insert(job.clone(), reason);
    }
    let full = found.everything.is_some() || found.rust || found.checks.is_some();
    let mut static_parts = vec![if full { "checks" } else { "light" }.to_owned()];
    if affected > 0 {
        static_parts.push("unit".to_owned());
    }
    let why = if let Some(reason) = &found.everything {
        format!("everything is selected: {reason}")
    } else if full {
        "Rust code or a compile-time input changed".to_owned()
    } else {
        "no Rust code changed: the checks that compile nothing".to_owned()
    };
    reasons.insert(
        "static".to_owned(),
        format!(
            "runs {} because {why}; {}",
            static_parts.join(" + "),
            unit.selection.line()
        ),
    );
    Decision {
        jobs,
        reasons,
        static_parts,
        unit,
    }
}
