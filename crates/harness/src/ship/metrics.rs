//! Revision-bound facts for the pull request metrics table, the `make metrics`
//! snapshot and the nightly regression issue. Static facts come from git blobs
//! at a commit ([`scan`]); measured ones from reports that already exist
//! ([`measured`]).
use super::git;
use std::{collections::BTreeMap, path::Path};

mod lexer;
pub(super) mod measured;
mod scan;
pub(super) mod snapshot;
mod table;

pub(super) use scan::{Metrics, repository};

/// Assign each path to the table's stable line-change class.
pub(super) fn class(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "Cargo.lock" | "package-lock.json") || path == "docs/gates.md" {
        "generated"
    } else if path.contains("/tests/") || path.ends_with("/tests.rs") || path.starts_with("tests/")
    {
        "tests"
    } else if path.starts_with("crates/harness/") || path.starts_with(".claude/") {
        "harness"
    } else if path.starts_with("crates/") && path.contains("/src/") {
        "production"
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        "docs"
    } else {
        "config"
    }
}

pub(super) fn scan_rust(source: &str) -> (u64, u64, u64) {
    lexer::scan_rust(source)
}

#[cfg(test)]
pub(super) fn comment_lines(source: &str) -> u64 {
    lexer::comment_lines(source)
}

pub(super) type LineCounts = (Option<u64>, Option<u64>);

fn numstat(root: &Path, merge_base: &str) -> Result<BTreeMap<String, LineCounts>, String> {
    let text = git::git(root, &["diff", "--numstat", merge_base, "HEAD"])?;
    let mut changed = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\t');
        let added = fields.next().and_then(|value| value.parse::<u64>().ok());
        let removed = fields.next().and_then(|value| value.parse::<u64>().ok());
        if let Some(path) = fields.next() {
            let counts = changed
                .entry(class(path).to_owned())
                .or_insert((Some(0), Some(0)));
            counts.0 = counts.0.zip(added).map(|(total, value)| total + value);
            counts.1 = counts.1.zip(removed).map(|(total, value)| total + value);
        }
    }
    Ok(changed)
}

pub(super) fn table(
    root: &Path,
    base: &str,
    rows: super::levels::RowSet,
) -> Result<String, String> {
    let (_, merge_base) = git::base(root, base, true)?;
    let before = snapshot::compute(root, &merge_base)?;
    let after = snapshot::compute(root, "HEAD")?;
    table::render(&before, &after, &numstat(root, &merge_base)?, rows)
}
