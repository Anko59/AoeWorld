//! The browser spec filter: `AOE_E2E_SPECS` runs only the named specs (or globs)
//! under `browser/tests`, as a focused check. It is never the CI suite.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The Playwright JSON report of a filtered run, written by the driver itself.
pub(super) const FILTERED_REPORT: &str = "reports/e2e/filtered.json";

/// Splits `AOE_E2E_SPECS` on spaces, tabs, newlines and commas.
pub(super) fn parse_specs(text: &str) -> Vec<String> {
    text.split(|character: char| character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A filtered run can never count as the full CI e2e, so CI refuses it.
/// Empty values count as unset, as the other variables do.
pub(super) fn refuse_in_ci(
    specs: &[String],
    github_actions: Option<&str>,
    ci: Option<&str>,
) -> Result<(), String> {
    let set = |value: Option<&str>| value.is_some_and(|value| !value.is_empty());
    if !specs.is_empty() && (set(github_actions) || set(ci)) {
        return Err(
            "AOE_E2E_SPECS runs part of the suite and is refused in CI (GITHUB_ACTIONS or CI is set): \
             a filtered run must never count as the full CI e2e; unset AOE_E2E_SPECS"
                .into(),
        );
    }
    Ok(())
}

/// The specs the patterns select from `candidates`, in pattern order, each once.
/// A pattern that selects nothing fails the run and names itself.
pub(super) fn resolve(candidates: &[String], patterns: &[String]) -> Result<Vec<String>, String> {
    let mut found: Vec<String> = Vec::new();
    for pattern in patterns {
        let matched: Vec<&String> = candidates
            .iter()
            .filter(|candidate| glob_matches(pattern, candidate))
            .collect();
        if matched.is_empty() {
            return Err(format!(
                "AOE_E2E_SPECS {pattern:?} matches no spec under browser/tests"
            ));
        }
        for spec in matched {
            if !found.contains(spec) {
                found.push(spec.clone());
            }
        }
    }
    Ok(found)
}

/// Every `*.spec.ts` under `browser/tests`, as paths relative to `browser/`.
pub(super) fn candidates(browser: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut found = Vec::new();
    collect(&browser.join("tests"), &mut found)?;
    let mut specs: Vec<String> = found
        .iter()
        .filter_map(|path| path.strip_prefix(browser).ok())
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .collect();
    specs.sort();
    Ok(specs)
}

fn collect(directory: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path.to_string_lossy().ends_with(".spec.ts") {
            found.push(path);
        }
    }
    Ok(())
}

/// The reports a finished run must have left behind. A filtered run keeps only
/// its own report: release evidence and the renderer evidence of the three
/// projects need the whole suite, so the full-suite checks in `run_specs` decide
/// an unfiltered run and this returns `None` for it.
pub(super) fn expected_report(specs: &[String]) -> Option<&'static str> {
    (!specs.is_empty()).then_some(FILTERED_REPORT)
}

/// Fails unless the report exists and holds something.
pub(super) fn check_report(root: &Path, report: &str) -> Result<(), String> {
    match fs::metadata(root.join(report)) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(()),
        _ => Err(format!("filtered run left no report: {report}")),
    }
}

/// Matches `pattern` against a spec path: `*` and `?` stay inside one directory,
/// `**` may cross directories.
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let path: Vec<char> = path.chars().collect();
    matches_from(&pattern, &path)
}

fn matches_from(pattern: &[char], path: &[char]) -> bool {
    match pattern {
        [] => path.is_empty(),
        ['*', '*', rest @ ..] => {
            let rest = rest.strip_prefix(&['/']).unwrap_or(rest);
            (0..=path.len()).any(|skip| matches_from(rest, &path[skip..]))
        }
        ['*', rest @ ..] => (0..=path.len())
            .take_while(|&skip| skip == 0 || path[skip - 1] != '/')
            .any(|skip| matches_from(rest, &path[skip..])),
        ['?', rest @ ..] => {
            path.first().is_some_and(|&character| character != '/')
                && matches_from(rest, &path[1..])
        }
        [literal, rest @ ..] => path.first() == Some(literal) && matches_from(rest, &path[1..]),
    }
}

#[cfg(test)]
mod tests;
