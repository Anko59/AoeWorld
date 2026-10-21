//! Diagnostics describe completed outcomes independently of quarantine eligibility.
use super::Result;
use serde_json::Value;
use std::collections::BTreeSet;

const LIMIT: usize = 40;

struct Entry {
    id: String,
    retry_name: String,
    detail: String,
    gap: bool,
    problem: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("unknown")
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn first_error(error: &Value) -> String {
    let message = error["message"]
        .as_str()
        .or_else(|| error["value"].as_str())
        .or_else(|| error["stack"].as_str())
        .unwrap_or("no error message");
    let message = crate::ship::run::strip_ansi(message);
    let line = message.lines().next().unwrap_or_default();
    let mut short: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        short.push('…');
    }
    short
}

fn collect(suite: &Value, parents: &[String], entries: &mut Vec<Entry>) {
    for spec in array(&suite["specs"]) {
        let mut titles = parents.to_vec();
        titles.push(text(spec, "title").to_owned());
        for test in array(&spec["tests"]) {
            let id = array(&test["annotations"])
                .iter()
                .find(|a| a["type"] == "aoe-quarantine-id")
                .and_then(|a| a["description"].as_str())
                .or_else(|| spec["id"].as_str())
                .unwrap_or_default()
                .to_owned();
            let name = format!(
                "{}:{} [{}] {}",
                text(spec, "file"),
                spec["line"],
                text(test, "projectName"),
                titles.join(" ")
            );
            let result = array(&test["results"]).last();
            let status = result.map_or("missing final result", |r| text(r, "status"));
            let gap = result.is_none() || matches!(status, "interrupted" | "unknown");
            let duration = result
                .and_then(|r| r["duration"].as_u64())
                .map_or_else(|| "unknown".to_owned(), |ms| format!("{ms}ms"));
            let error = result.map(|r| {
                r.get("error")
                    .filter(|error| !error.is_null())
                    .or_else(|| array(&r["errors"]).first())
                    .unwrap_or(&Value::Null)
            });
            entries.push(Entry {
                id,
                retry_name: format!(
                    "{} [{}] {}",
                    text(spec, "file")
                        .strip_suffix(".spec.ts")
                        .unwrap_or(text(spec, "file")),
                    text(test, "projectName"),
                    titles.join(" ")
                ),
                detail: format!(
                    "{name} | status={status} | duration={duration} | {}",
                    first_error(error.unwrap_or(&Value::Null))
                ),
                gap,
                problem: gap || test["status"] == "unexpected",
            });
        }
    }
    for child in array(&suite["suites"]) {
        let mut titles = parents.to_vec();
        let title = text(child, "title");
        if !title.ends_with(".spec.ts") && !title.is_empty() {
            titles.push(title.to_owned());
        }
        collect(child, &titles, entries);
    }
}

/// Preserve strict validation errors, adding the available named outcomes even
/// when malformed evidence prevents recovery. `unresolved` excludes recovered cases.
pub(super) fn failure(
    value: &Value,
    reason: &str,
    unresolved: Option<&[String]>,
) -> Box<dyn std::error::Error> {
    let mut entries = Vec::new();
    collect(value, &[], &mut entries);
    let evidence = &value["config"]["metadata"]["aoeQuarantine"];
    let seen: BTreeSet<_> = entries.iter().map(|entry| entry.id.clone()).collect();
    for id in array(&evidence["selected"])
        .iter()
        .filter_map(Value::as_str)
    {
        if seen.contains(id) {
            continue;
        }
        let test = &evidence["tests"][id];
        let name = if test.is_object() {
            format!(
                "{}:{} [{}] {}",
                text(test, "file"),
                test["line"],
                text(test, "project"),
                text(test, "title")
            )
        } else {
            // If identity metadata is absent, retain the observed ID.
            format!("selected test {id}")
        };
        entries.push(Entry {
            id: id.to_owned(),
            retry_name: String::new(),
            detail: format!("{name} | status=missing final result"),
            gap: true,
            problem: true,
        });
    }
    let globals = array(&value["errors"]);
    let interrupted = entries.iter().any(|entry| entry.gap) || !globals.is_empty();
    let heading = if interrupted { "interrupted" } else { "failed" };
    let mut lines = vec![format!("Playwright {heading}: {reason}")];
    let problems: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry.problem && unresolved.is_none_or(|names| names.contains(&entry.retry_name))
        })
        .collect();
    lines.extend(
        problems
            .iter()
            .take(LIMIT)
            .map(|entry| format!("  {}", entry.detail)),
    );
    if problems.len() > LIMIT {
        lines.push(format!("  … {} more tests", problems.len() - LIMIT));
    }
    lines.extend(
        globals
            .iter()
            .take(LIMIT)
            .map(|error| format!("  global error: {}", first_error(error))),
    );
    if globals.len() > LIMIT {
        lines.push(format!("  … {} more global errors", globals.len() - LIMIT));
    }
    let stats = &value["stats"];
    lines.push(format!(
        "expected={} unexpected={} skipped={} flaky={}",
        stats["expected"], stats["unexpected"], stats["skipped"], stats["flaky"]
    ));
    lines.join("\n").into()
}

pub(super) fn checked<T>(value: &Value, result: Result<T>) -> Result<T> {
    result.map_err(|error| failure(value, &error.to_string(), None))
}
