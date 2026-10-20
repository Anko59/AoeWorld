//! Parse bounded Playwright JSON for focused duel checks.
use super::{CheckReport, output};
use crate::ship::git;
use std::path::Path;

pub(in crate::ship) fn browser_results(path: &Path) -> git::Result<CheckReport> {
    let value: serde_json::Value =
        serde_json::from_slice(&output::read(path, "Playwright")?).map_err(|e| e.to_string())?;
    fn walk(v: &serde_json::Value, parents: &str, report: &mut CheckReport) {
        if let Some(specs) = v["specs"].as_array() {
            for spec in specs {
                let file = spec["file"].as_str().unwrap_or("unknown");
                let title = spec["title"].as_str().unwrap_or("unknown");
                if let Some(tests) = spec["tests"].as_array() {
                    for test in tests {
                        let name = format!(
                            "{file}::{parents}{title} [{}]",
                            test["projectName"].as_str().unwrap_or("unknown")
                        );
                        // `expected` is an outcome, not a pass: test.fail() also earns it.
                        // Only an observed pass with a passing expectation is green evidence.
                        let passed = test["status"] == "expected"
                            && test["expectedStatus"] == "passed"
                            && test["results"]
                                .as_array()
                                .and_then(|results| results.last())
                                .is_some_and(|result| result["status"] == "passed");
                        let failed =
                            !matches!(test["status"].as_str(), Some("expected" | "skipped"));
                        if passed || failed {
                            report.tests.push(name.clone());
                        }
                        report.locations.insert(
                            name.clone(),
                            (
                                if file.starts_with("tests/") {
                                    format!("browser/{file}")
                                } else {
                                    format!("browser/tests/{file}")
                                },
                                spec["line"].as_u64().unwrap_or(0) as usize,
                            ),
                        );
                        if failed {
                            report.failed.push(name);
                        }
                    }
                }
            }
        }
        if let Some(suites) = v["suites"].as_array() {
            for suite in suites {
                let title = suite["title"].as_str().unwrap_or("");
                let parents = if title == suite["file"].as_str().unwrap_or("") {
                    parents.to_owned()
                } else {
                    format!("{parents}{title}::")
                };
                walk(suite, &parents, report);
            }
        }
    }
    let mut report = CheckReport::default();
    walk(&value, "", &mut report);
    if collection_errors(&value) || report.locations.is_empty() {
        return Err("Playwright errors or no focused tests".into());
    }
    report.passed = report.failed.is_empty();
    Ok(report)
}

fn collection_errors(value: &serde_json::Value) -> bool {
    value["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
        || value.get("error").is_some_and(|error| !error.is_null())
        || value["suites"]
            .as_array()
            .is_some_and(|suites| suites.iter().any(collection_errors))
}
