//! A changed Rust test that does not compile because it calls an API the PR adds is red.
use super::{git, is_test};
use crate::ship::duel::focus::CheckReport;
use std::{collections::BTreeSet, path::Path};

/// Error codes that name an item the compiler cannot find.
const MISSING: &[&str] = &[
    "E0405", "E0412", "E0422", "E0423", "E0425", "E0432", "E0433", "E0531", "E0560", "E0599",
    "E0609",
];

/// The items of the PR's added product code that the red build could not find. Every compile
/// error must sit in a changed test file. At least one must be a miss (`MISSING`) naming an
/// identifier the PR adds; the others in changed test files are its cascade: inference and
/// method errors on values built from the missing items (E0282, E0283, E0599 on an unknown
/// type...). Each such file must still have a test that runs and passes at head (`ran`).
pub(super) fn missing(
    root: &Path,
    base: &str,
    head: &str,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<Vec<String>> {
    if report.unavailable.is_some() || report.compile_errors.is_empty() {
        return Err(
            "red: the build failed without readable compile errors; not a named failing test"
                .into(),
        );
    }
    let added = added_identifiers(root, base, head)?;
    let refuse = |code: Option<&str>, path: &str| {
        format!(
            "red: build error {} at {path} is not a changed test calling an API the PR adds",
            code.unwrap_or("(no code)")
        )
    };
    if let Some(outside) = report
        .compile_errors
        .iter()
        .find(|e| !paths.contains(&e.path))
    {
        return Err(refuse(outside.code.as_deref(), &outside.path));
    }
    let mut missing = BTreeSet::new();
    for error in &report.compile_errors {
        if error.code.as_deref().is_some_and(|c| MISSING.contains(&c)) {
            missing.extend(
                error
                    .items
                    .iter()
                    .filter(|item| added.contains(last_segment(item)))
                    .cloned(),
            );
        }
    }
    if missing.is_empty() {
        let first = &report.compile_errors[0];
        return Err(refuse(first.code.as_deref(), &first.path));
    }
    Ok(missing.into_iter().collect())
}

fn last_segment(item: &str) -> &str {
    item.rsplit("::")
        .next()
        .unwrap_or(item)
        .trim_matches(['{', '}', ' '])
}

/// Identifiers on lines the PR adds to files that are not tests.
fn added_identifiers(root: &Path, base: &str, head: &str) -> git::Result<BTreeSet<String>> {
    let patch = git::git_raw(root, &["diff", "-U0", "--no-renames", base, head])?;
    let mut identifiers = BTreeSet::new();
    let mut product = false;
    for line in patch.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            product = !is_test(path);
        } else if line.starts_with("+++ ") {
            product = false;
        } else if product && let Some(added) = line.strip_prefix('+') {
            identifiers.extend(
                added
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|word| !word.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    Ok(identifiers)
}

/// The changed tests that ran and passed at head, at least one in each file that did not compile
/// at red: a file whose tests are filtered, ignored or unmounted at head proves nothing.
pub(super) fn ran(
    green_tree: &Path,
    base_tree: &Path,
    red: &CheckReport,
    green: &CheckReport,
) -> git::Result<Vec<String>> {
    let files: BTreeSet<&str> = red.compile_errors.iter().map(|e| e.path.as_str()).collect();
    let mut ran = BTreeSet::new();
    for file in files {
        let mut found = false;
        for name in green.tests.iter().filter(|n| !green.failed.contains(n)) {
            if crate::ship::duel::authored::changed_failure(
                green_tree,
                base_tree,
                &[file.to_owned()],
                name,
            )? {
                ran.insert(name.clone());
                found = true;
            }
        }
        if !found {
            return Err(format!(
                "red: no changed test of {file} ran and passed at HEAD (filtered, ignored or not mounted?)"
            ));
        }
    }
    Ok(ran.into_iter().collect())
}
