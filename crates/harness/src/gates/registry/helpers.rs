use std::collections::{BTreeMap, BTreeSet};

pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
}

pub(super) fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

pub(super) fn valid_pattern(pattern: &str) -> bool {
    valid_path(pattern)
        && !pattern.contains(['[', ']', '{', '}', '(', ')', '|', '^', '$', '+'])
        && pattern.split('/').all(|s| !s.contains("**") || s == "**")
}

pub(super) fn instruction_path(path: &str) -> bool {
    path == "docs/agent-engineering.md" || path == "AGENTS.md" || path.ends_with("/AGENTS.md")
}

pub(super) fn duplicates<T: Ord>(values: &[T], owner: &str, problems: &mut Vec<String>) {
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        problems.push(format!("duplicate list member in {owner}"));
    }
}

pub(super) fn references(
    values: &[String],
    known: &BTreeSet<String>,
    owner: &str,
    kind: &str,
    problems: &mut Vec<String>,
) {
    for value in values {
        if !known.contains(value) {
            problems.push(format!("unknown {kind} {value} in {owner}"));
        }
    }
}

pub(super) fn dependency_closure(
    initial: &BTreeSet<String>,
    graph: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut result = initial.clone();
    let mut pending: Vec<_> = initial.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        if let Some(deps) = graph.get(&id) {
            for dep in deps {
                if result.insert(dep.clone()) {
                    pending.push(dep.clone());
                }
            }
        }
    }
    result
}

pub(super) fn unresolved(
    graph: &BTreeMap<String, Vec<String>>,
    kind: &str,
    problems: &mut Vec<String>,
) {
    let mut done = BTreeSet::new();
    loop {
        let before = done.len();
        for (id, deps) in graph {
            if deps.iter().all(|dep| done.contains(dep)) {
                done.insert(id.clone());
            }
        }
        if before == done.len() {
            break;
        }
    }
    for id in graph.keys().filter(|id| !done.contains(*id)) {
        problems.push(format!("cyclic or unresolved {kind} {id}"));
    }
}
