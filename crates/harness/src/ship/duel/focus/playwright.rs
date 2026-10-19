//! Parse bounded Playwright JSON; load failures are exposed only to the red runner.
use super::{CheckReport, output};
use crate::ship::git;
use std::path::Path;

pub(in crate::ship) fn browser_results(path: &Path) -> git::Result<CheckReport> {
    results(path, None)
}

pub(in crate::ship) fn browser_results_red(path: &Path, root: &Path) -> git::Result<CheckReport> {
    results(path, Some(root))
}

fn results(path: &Path, red_root: Option<&Path>) -> git::Result<CheckReport> {
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
                        if test["status"] != "skipped" {
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
                        if !matches!(test["status"].as_str(), Some("expected" | "skipped")) {
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
    let mut errors = Vec::new();
    collection_errors(&value, None, &mut errors);
    if let Some(root) = red_root {
        for (error, suite) in &errors {
            if error["message"]
                .as_str()
                .is_some_and(|s| s.starts_with("Error: No tests found."))
            {
                continue;
            }
            report
                .compile_errors
                .push(load_error(root, error, *suite).ok_or(
                    "Playwright collection error is not an attributable module/export load failure",
                )?);
        }
    }
    if report.compile_errors.is_empty() && (!errors.is_empty() || report.locations.is_empty()) {
        return Err("Playwright errors or no focused tests".into());
    }
    report.build_failed = !report.compile_errors.is_empty();
    report.passed = !report.build_failed && report.failed.is_empty();
    Ok(report)
}

fn collection_errors<'a>(
    value: &'a serde_json::Value,
    file: Option<&'a str>,
    errors: &mut Vec<(&'a serde_json::Value, Option<&'a str>)>,
) {
    let file = value["file"].as_str().or(file);
    if let Some(list) = value["errors"].as_array() {
        errors.extend(list.iter().map(|error| (error, file)));
    }
    if value.get("error").is_some_and(|e| !e.is_null()) {
        errors.push((&value["error"], file));
    }
    if let Some(suites) = value["suites"].as_array() {
        for suite in suites {
            collection_errors(suite, file, errors);
        }
    }
}

fn load_error(
    root: &Path,
    error: &serde_json::Value,
    suite: Option<&str>,
) -> Option<super::compile::CompileError> {
    let message = error["message"].as_str()?;
    let (code, item) =
        if let Some((_, rest)) = message.split_once("does not provide an export named ") {
            ("JS_EXPORT", quoted(rest)?)
        } else if let Some((_, rest)) = message.split_once("Cannot find module ") {
            ("JS_MODULE", quoted(rest)?)
        } else if let Some((_, rest)) = message.split_once("Failed to load url ") {
            (
                "JS_MODULE",
                if rest.starts_with(['\'', '"', '`']) {
                    quoted(rest)?
                } else {
                    rest.split_whitespace().next()?
                },
            )
        } else if message.contains("ENOENT")
            && let Some((_, rest)) = message.split_once("copyfile ")
        {
            ("JS_MODULE", quoted(rest)?)
        } else {
            return None;
        };
    let path = match error["location"]["file"].as_str() {
        // Playwright's ESM resolver reports an eval frame, not a source file.
        // It still names the importer in the message; never infer it from a suite alone.
        Some("<anonymous>") => stack_path(root, message)
            .or_else(|| stack_path(root, error["stack"].as_str().unwrap_or("")))?,
        Some(file) => source_path(root, file)?,
        None => stack_path(root, error["stack"].as_str().unwrap_or(message))
            .or_else(|| suite.and_then(|p| source_path(root, p)))?,
    };
    let item = if code == "JS_MODULE" {
        let path = if item.starts_with("file://") {
            url::Url::parse(item).ok()?.to_file_path().ok()?
        } else if Path::new(item).is_absolute() {
            item.into()
        } else if item.starts_with('.') {
            root.join(Path::new(&path).parent()?).join(item)
        } else {
            return None;
        };
        let normalized = crate::ship::duel::wasm::normalize(&path.to_string_lossy());
        Path::new(&normalized)
            .strip_prefix(root)
            .ok()?
            .to_str()?
            .to_owned()
    } else {
        item.to_owned()
    };
    Some(super::compile::CompileError {
        path,
        code: Some(code.into()),
        items: vec![item],
    })
}

fn quoted(text: &str) -> Option<&str> {
    let quote = text.chars().next()?;
    if !matches!(quote, '\'' | '"' | '`') {
        return None;
    }
    text[1..].split_once(quote).map(|(item, _)| item)
}

fn source_path(root: &Path, path: &str) -> Option<String> {
    if Path::new(path).is_absolute() {
        return Path::new(path)
            .strip_prefix(root)
            .ok()?
            .to_str()
            .map(str::to_owned);
    }
    Some(if path.starts_with("browser/") {
        path.to_owned()
    } else if path.starts_with("tests/") {
        format!("browser/{path}")
    } else {
        format!("browser/tests/{path}")
    })
}

fn stack_path(root: &Path, stack: &str) -> Option<String> {
    for line in stack.lines() {
        let line = line.trim();
        let source = if let Some((_, importer)) = line
            .split_once(" imported from ")
            .or_else(|| line.split_once(" from "))
        {
            importer
        } else if line.starts_with("at ") || line.starts_with("- ") {
            line
        } else {
            continue;
        };
        for word in source.split_whitespace() {
            let word = word.trim_matches(['\'', '"', '(', ')', ',']);
            let path = word
                .strip_prefix("file://")
                .unwrap_or(word)
                .split(':')
                .next()?;
            if Path::new(path).is_absolute() {
                // Preserve an outside frame so it is refused, rather than falling back
                // to a suite filename that happens to be in the changed scope.
                return Some(source_path(root, path).unwrap_or_else(|| path.to_owned()));
            }
        }
    }
    None
}
