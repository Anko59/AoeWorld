//! Revision-bound facts for the optional pull request metrics table.
use super::git;
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub(super) struct Metrics {
    pub(super) lines: BTreeMap<String, u64>,
    pub(super) tests: BTreeMap<String, u64>,
    pub(super) comments: BTreeMap<String, u64>,
}

/// Assign each path to the table's stable line-change class.
pub(super) fn class(path: &str) -> &'static str {
    if path.starts_with("crates/harness/") || path.starts_with(".claude/") {
        "harness"
    } else if path.contains("/tests/") || path.ends_with("/tests.rs") || path.starts_with("tests/")
    {
        "tests"
    } else if path.starts_with("crates/") && path.contains("/src/") {
        "production"
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        "docs"
    } else {
        "config"
    }
}

fn source_paths(root: &Path, revision: &str) -> Result<Vec<(String, String)>, String> {
    let listing = git::git(root, &["ls-tree", "-rz", "--full-tree", revision])?;
    Ok(listing
        .split('\0')
        .filter_map(|entry| {
            let (metadata, path) = entry.split_once('\t')?;
            let mut fields = metadata.split_whitespace();
            let mode = fields.next()?;
            let kind = fields.next()?;
            let oid = fields.next()?;
            (kind == "blob" && mode != "120000" && path.ends_with(".rs"))
                .then(|| (path.to_owned(), oid.to_owned()))
        })
        .collect())
}

/// Read batch output while a separate thread feeds IDs, so neither pipe can
/// fill while the other side waits for progress.
fn blobs(root: &Path, objects: Vec<(String, String)>) -> Result<Metrics, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("git cat-file --batch: {error}"))?;
    let mut input = child.stdin.take().ok_or("cat-file stdin is unavailable")?;
    let ids: Vec<String> = objects.iter().map(|(_, oid)| oid.clone()).collect();
    let feeder = thread::spawn(move || -> io::Result<()> {
        for oid in ids {
            writeln!(input, "{oid}")?;
        }
        Ok(())
    });
    let stdout = child
        .stdout
        .take()
        .ok_or("cat-file stdout is unavailable")?;
    let scan = read_blobs(BufReader::new(stdout), &objects);
    if scan.is_err() {
        let _ = child.kill();
    }
    let feed_result = feeder
        .join()
        .map_err(|_| "cat-file feeder thread panicked".to_owned())?
        .map_err(|error| format!("feeding cat-file object IDs: {error}"));
    let status = child
        .wait()
        .map_err(|error| format!("waiting for cat-file: {error}"))?;
    let metrics = scan?;
    feed_result?;
    if !status.success() {
        return Err(format!("git cat-file --batch exited with {status}"));
    }
    Ok(metrics)
}

fn read_blobs(mut output: impl BufRead, objects: &[(String, String)]) -> Result<Metrics, String> {
    let mut metrics = Metrics::default();
    for (path, expected_oid) in objects {
        let mut header = String::new();
        output
            .read_line(&mut header)
            .map_err(|error| format!("reading cat-file header: {error}"))?;
        let mut fields = header.split_whitespace();
        let oid = fields.next().unwrap_or_default();
        let kind = fields.next().unwrap_or_default();
        let size = fields
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| format!("invalid cat-file header for {path}"))?;
        if oid != expected_oid || kind != "blob" {
            return Err(format!("unexpected cat-file object for {path}"));
        }
        if size <= MAX_SOURCE_BYTES {
            let mut bytes = vec![0; size];
            output
                .read_exact(&mut bytes)
                .map_err(|error| format!("reading blob {path}: {error}"))?;
            let mut delimiter = [0];
            output
                .read_exact(&mut delimiter)
                .map_err(|error| format!("reading blob delimiter {path}: {error}"))?;
            if delimiter[0] != b'\n' {
                return Err(format!("invalid cat-file delimiter for {path}"));
            }
            if !bytes.contains(&0)
                && let Ok(source) = std::str::from_utf8(&bytes)
            {
                accumulate_source(&mut metrics, path, source);
            }
        } else {
            io::copy(&mut output.by_ref().take(size as u64), &mut io::sink())
                .map_err(|error| format!("draining large blob {path}: {error}"))?;
            let mut delimiter = [0];
            output
                .read_exact(&mut delimiter)
                .map_err(|error| format!("reading large blob delimiter {path}: {error}"))?;
            if delimiter[0] != b'\n' {
                return Err(format!("invalid cat-file delimiter for {path}"));
            }
        }
    }
    Ok(metrics)
}

/// Snapshot only tracked Rust text at a commit or merge-base revision.
pub(super) fn repository(root: &Path, revision: &str) -> Result<Metrics, String> {
    let objects = source_paths(root, revision)?;
    blobs(root, objects)
}

fn accumulate_source(metrics: &mut Metrics, path: &str, source: &str) {
    let category = class(path).to_owned();
    let (lines, comments, attributes) = scan_rust(source);
    *metrics.lines.entry(category.clone()).or_default() += lines;
    *metrics.comments.entry(category).or_default() += comments;
    let integration = path.starts_with("crates/") && path.split('/').nth(2) == Some("tests");
    let layer = if integration { "integration" } else { "unit" };
    *metrics.tests.entry(layer.to_owned()).or_default() += attributes;
}

mod lexer;

pub(super) fn scan_rust(source: &str) -> (u64, u64, u64) {
    lexer::scan_rust(source)
}

#[cfg(test)]
pub(super) fn comment_lines(source: &str) -> u64 {
    lexer::comment_lines(source)
}

type LineCounts = (Option<u64>, Option<u64>);

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

fn density(metrics: &Metrics) -> Option<f64> {
    let lines = metrics.lines.get("production").copied().unwrap_or(0);
    let comments = metrics.comments.get("production").copied().unwrap_or(0);
    if lines == 0 {
        None
    } else {
        Some(comments as f64 * 100.0 / lines as f64)
    }
}

fn count(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| value.to_string())
}

fn signed_delta(before: u64, after: u64) -> String {
    if after >= before {
        format!("+{}", after - before)
    } else {
        format!("−{}", before - after)
    }
}

pub(super) fn table(root: &Path) -> Result<String, String> {
    let (_, merge_base) = git::base(root, "dev", true)?;
    let base = repository(root, &merge_base)?;
    let head = repository(root, "HEAD")?;
    let changed = numstat(root, &merge_base)?;
    let mut out = String::from("| 📊 Metric | merge base | this PR | |\n|---|---:|---:|---:|\n");
    for category in ["production", "tests", "harness", "docs", "config"] {
        let (added, removed) = changed.get(category).copied().unwrap_or((Some(0), Some(0)));
        out.push_str(&format!(
            "| Lines changed: {category} | | +{} / −{} | |\n",
            count(added),
            count(removed)
        ));
    }
    for layer in ["unit", "integration"] {
        let before = base.tests.get(layer).copied().unwrap_or(0);
        let after = head.tests.get(layer).copied().unwrap_or(0);
        out.push_str(&format!(
            "| Tests: {layer} (source #[test] attributes) | {before} | {after} ({}) | 🟢 |\n",
            signed_delta(before, after)
        ));
    }
    let density_text =
        |value: Option<f64>| value.map_or_else(|| "—".to_owned(), |n| format!("{n:.1}"));
    out.push_str(&format!(
        "| Comment density in production code (%) | {} | {} | |\n",
        density_text(density(&base)),
        density_text(density(&head))
    ));
    let before = base.lines.get("production").copied();
    let after = head.lines.get("production").copied();
    let delta = match (before, after) {
        (Some(before), Some(after)) => signed_delta(before, after),
        _ => "—".to_owned(),
    };
    out.push_str(&format!(
        "| Production lines | {} | {} ({delta}) | |\n",
        count(before),
        count(after)
    ));
    Ok(out)
}
