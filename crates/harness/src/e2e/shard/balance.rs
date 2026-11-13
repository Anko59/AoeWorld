//! Stable Playwright identities and longest-processing-time scheduling.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) type Durations = BTreeMap<String, u64>;

/// Playwright 1.64.1 `loadTestList` matches project, root-relative file and title
/// components. It ignores line numbers and accepts title prefixes. Keep full
/// paths and verify the filtered discovery before trusting that selection.
pub(super) fn records(report: &Value) -> Result<Vec<(String, u64)>> {
    if !report["errors"].as_array().is_some_and(Vec::is_empty) {
        return Err("Playwright discovery/report has missing or global errors".into());
    }
    let mut cases = Vec::new();
    for file in report["suites"].as_array().ok_or("missing suites")? {
        collect(file, &[], &mut cases)?;
    }
    let names: Vec<_> = cases.iter().map(|(name, _)| name.clone()).collect();
    check_selection(&names, &names)?;
    Ok(cases)
}

fn component(value: &Value, key: &str) -> Result<String> {
    let text = value[key]
        .as_str()
        .ok_or_else(|| format!("missing {key}"))?;
    if text.is_empty() || text.trim() != text || text.contains(['›', '\n', '\r']) {
        return Err(format!("unrepresentable test-list {key}: {text:?}").into());
    }
    Ok(text.to_owned())
}

fn collect(suite: &Value, parents: &[String], cases: &mut Vec<(String, u64)>) -> Result<()> {
    if let Some(specs) = suite.get("specs") {
        for spec in specs.as_array().ok_or("invalid specs")? {
            let file = component(spec, "file")?;
            let mut titles = parents.to_vec();
            titles.push(component(spec, "title")?);
            for test in spec["tests"].as_array().ok_or("missing tests")? {
                let project = component(test, "projectName")?;
                if project.contains(['[', ']']) {
                    return Err("unrepresentable test-list project".into());
                }
                let name = format!("[{project}] › {file} › {}", titles.join(" › "));
                let mut duration = 0u64;
                for result in test["results"].as_array().ok_or("missing results")? {
                    duration = duration
                        .checked_add(result["duration"].as_u64().ok_or("missing duration")?)
                        .ok_or("duration overflow")?;
                }
                cases.push((name, duration));
            }
        }
    }
    if let Some(children) = suite.get("suites") {
        for child in children.as_array().ok_or("invalid suites")? {
            let mut titles = parents.to_vec();
            titles.push(component(child, "title")?);
            collect(child, &titles, cases)?;
        }
    }
    Ok(())
}

pub(super) fn check_selection(expected: &[String], actual: &[String]) -> Result<()> {
    let wanted: BTreeSet<_> = expected.iter().collect();
    let found: BTreeSet<_> = actual.iter().collect();
    if wanted.len() != expected.len() || found.len() != actual.len() || wanted != found {
        return Err(format!(
            "shard selection differs: expected {} unique tests, got {}; missing {:?}; extra {:?}",
            expected.len(),
            actual.len(),
            wanted.difference(&found).collect::<Vec<_>>(),
            found.difference(&wanted).collect::<Vec<_>>()
        )
        .into());
    }
    Ok(())
}

pub(super) fn assign(
    tests: &[String],
    durations: &Durations,
    count: u32,
) -> Result<Vec<Vec<String>>> {
    if !(1..=16).contains(&count) || tests.is_empty() {
        return Err("shard plan needs tests and 1..=16 shards".into());
    }
    check_selection(tests, tests)?;
    let mut measured: Vec<_> = durations.values().copied().collect();
    measured.sort_unstable();
    // Use integer seconds throughout; an empty history defaults to one second.
    let median = if measured.is_empty() {
        1
    } else {
        let middle = measured.len() / 2;
        let lower = measured[(measured.len() - 1) / 2];
        lower + (measured[middle] - lower) / 2
    };
    let mut weighted: Vec<_> = tests
        .iter()
        .map(|test| (test, durations.get(test).copied().unwrap_or(median)))
        .collect();
    weighted.sort_by(|(a, wa), (b, wb)| wb.cmp(wa).then_with(|| a.cmp(b)));
    let mut loads = vec![0u64; count as usize];
    let mut shards = vec![Vec::new(); count as usize];
    for (test, weight) in weighted {
        let index = (0..loads.len())
            .min_by_key(|&index| (loads[index], index))
            .ok_or("no shards")?;
        loads[index] = loads[index]
            .checked_add(weight)
            .ok_or("shard duration overflow")?;
        shards[index].push(test.clone());
    }
    for tests in &mut shards {
        tests.sort();
    }
    Ok(shards)
}
