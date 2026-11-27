//! The checks `make ship` runs on the agent's machine and CI does not (`local_checks` of
//! `gates/ci-rules.json`): the game score. They use the crate graph the way a job does, so one
//! answer to "can this change affect X" serves both, but they are not jobs: CI's manifest never
//! lists them.
use super::{
    graph::Graph,
    rules::{LocalRule, Rules, any},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalDecision {
    pub(crate) runs: bool,
    /// "runs because ..." or "skipped because ...".
    pub(crate) reason: String,
    /// The tracked paths whose content decides the check's result: the directories of the
    /// workspace crates in the closure, then the rule's own inputs. Sorted, without duplicates.
    pub(crate) inputs: Vec<String>,
}

/// The workspace crates a build of the rule's subjects compiles, by name.
fn closure(rule: &LocalRule, graph: &Graph) -> BTreeSet<String> {
    rule.crates
        .iter()
        .flat_map(|subject| graph.closure(subject, rule.tests))
        .filter(|name| graph.is_workspace(name))
        .collect()
}

fn inputs(rule: &LocalRule, graph: &Graph, closure: &BTreeSet<String>) -> Vec<String> {
    let directories = graph
        .workspace()
        .iter()
        .filter(|package| closure.contains(&package.name))
        .map(|package| package.dir.clone());
    let all: BTreeSet<String> = directories.chain(rule.inputs.iter().cloned()).collect();
    all.into_iter().collect()
}

fn under(path: &str, input: &str) -> bool {
    path == input
        || path
            .strip_prefix(input)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Why `path` runs the check, if it does.
fn reason_for(
    rule: &LocalRule,
    graph: &Graph,
    closure: &BTreeSet<String>,
    path: &str,
) -> Option<String> {
    if any(&rule.ignore, path) {
        return None;
    }
    if rule.inputs.iter().any(|input| under(path, input)) {
        return Some(format!("{path} is an input of the game score"));
    }
    let name = graph.owner(path).filter(|name| closure.contains(name))?;
    let subject = rule
        .crates
        .iter()
        .find(|subject| graph.closure(subject, rule.tests).contains(&name))?;
    Some(if *subject == name {
        format!("{name} changed ({path})")
    } else {
        format!("{subject} builds on {name}, which changed ({path})")
    })
}

fn decide_one(rule: &LocalRule, graph: &Graph, paths: &[String]) -> LocalDecision {
    let closure = closure(rule, graph);
    let inputs = inputs(rule, graph, &closure);
    let (runs, reason) = if paths.is_empty() {
        (false, "skipped because no path changed".to_owned())
    } else if let Some(reason) = paths
        .iter()
        .find_map(|path| reason_for(rule, graph, &closure, path))
    {
        (true, format!("runs because {reason}"))
    } else {
        let crates = rule.crates.join(", ");
        (
            false,
            format!(
                "skipped because no changed crate is in the build closure of {crates} \
                 and no changed path is an input"
            ),
        )
    };
    LocalDecision {
        runs,
        reason,
        inputs,
    }
}

/// Every local check, decided from the changed `paths`.
pub(crate) fn decide_local(
    rules: &Rules,
    graph: &Graph,
    paths: &[String],
) -> BTreeMap<String, LocalDecision> {
    rules
        .local_checks
        .iter()
        .map(|(name, rule)| (name.clone(), decide_one(rule, graph, paths)))
        .collect()
}
