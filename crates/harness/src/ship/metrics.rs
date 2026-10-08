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
fn blobs(root: &Path, objects: Vec<(String, String)>) -> Result<Vec<(String, Vec<u8>)>, String> {
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
    let blobs = scan?;
    feed_result?;
    if !status.success() {
        return Err(format!("git cat-file --batch exited with {status}"));
    }
    Ok(blobs)
}

fn read_blobs(
    mut output: impl BufRead,
    objects: &[(String, String)],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut blobs = Vec::new();
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
            blobs.push((path.clone(), bytes));
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
    Ok(blobs)
}

/// Snapshot only tracked Rust text at a commit or merge-base revision.
pub(super) fn repository(root: &Path, revision: &str) -> Result<Metrics, String> {
    let objects = source_paths(root, revision)?;
    let mut metrics = Metrics::default();
    for (path, bytes) in blobs(root, objects)? {
        if bytes.contains(&0) {
            continue;
        }
        let Ok(source) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let category = class(&path).to_owned();
        *metrics.lines.entry(category.clone()).or_default() += source.lines().count() as u64;
        *metrics.comments.entry(category).or_default() += comment_lines(source);
        let layer = if path.starts_with("tests/") || path.contains("/tests/") {
            "integration"
        } else {
            "unit"
        };
        let attributes = source
            .lines()
            .filter(|line| {
                let line = line.trim();
                line == "#[test]" || line.starts_with("#[test(")
            })
            .count() as u64;
        *metrics.tests.entry(layer.to_owned()).or_default() += attributes;
    }
    Ok(metrics)
}

#[derive(Clone, Copy)]
enum LexState {
    Normal,
    String,
    Character,
    Raw(usize),
    LineComment,
    BlockComment(usize),
}

/// Count lines containing lexical Rust comments. Markers inside normal and raw
/// strings are ignored; macro-generated tokens and embedded foreign languages
/// are not parsed as Rust syntax.
pub(super) fn comment_lines(source: &str) -> u64 {
    let bytes = source.as_bytes();
    let mut state = LexState::Normal;
    let mut in_line = false;
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        match state {
            LexState::Normal => match bytes[i] {
                b'\n' => {
                    if in_line {
                        count += 1;
                    }
                    in_line = false;
                    i += 1;
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    state = LexState::LineComment;
                    in_line = true;
                    i += 2;
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    state = LexState::BlockComment(1);
                    in_line = true;
                    i += 2;
                }
                b'r' | b'b' => {
                    if let Some((hashes, end)) = raw_string_start(bytes, i) {
                        state = LexState::Raw(hashes);
                        i = end;
                    } else {
                        i += 1;
                    }
                }
                b'"' => {
                    state = LexState::String;
                    i += 1;
                }
                b'\'' if char_end(bytes, i).is_some() => {
                    state = LexState::Character;
                    i += 1;
                }
                _ => i += 1,
            },
            LexState::String | LexState::Character => {
                let delimiter = if matches!(state, LexState::String) {
                    b'"'
                } else {
                    b'\''
                };
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == delimiter {
                    state = LexState::Normal;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            LexState::Raw(hashes) => {
                if bytes[i] == b'"'
                    && bytes.get(i + 1..i + 1 + hashes) == Some(&vec![b'#'; hashes][..])
                {
                    state = LexState::Normal;
                    i += hashes + 1;
                } else {
                    i += 1;
                }
            }
            LexState::LineComment => {
                if bytes[i] == b'\n' {
                    count += 1;
                    in_line = false;
                    state = LexState::Normal;
                }
                i += 1;
            }
            LexState::BlockComment(depth) => {
                in_line = true;
                match (bytes[i], bytes.get(i + 1)) {
                    (b'/', Some(b'*')) => {
                        state = LexState::BlockComment(depth + 1);
                        i += 2;
                    }
                    (b'*', Some(b'/')) => {
                        state = if depth == 1 {
                            LexState::Normal
                        } else {
                            LexState::BlockComment(depth - 1)
                        };
                        i += 2;
                    }
                    (b'\n', _) => {
                        count += 1;
                        in_line = false;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
        }
    }
    if in_line || matches!(state, LexState::BlockComment(_)) {
        count += 1;
    }
    count
}

fn raw_string_start(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut i = start;
    if bytes.get(i) == Some(&b'b') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'r') {
        return None;
    }
    i += 1;
    let hash_start = i;
    while bytes.get(i) == Some(&b'#') {
        i += 1;
    }
    (bytes.get(i) == Some(&b'"')).then_some((i - hash_start, i + 1))
}

fn char_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b'\n' {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'\'' {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

fn numstat(root: &Path, merge_base: &str) -> Result<BTreeMap<String, (u64, u64)>, String> {
    let text = git::git(root, &["diff", "--numstat", merge_base, "HEAD"])?;
    let mut changed = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\t');
        let added = fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let removed = fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if let Some(path) = fields.next() {
            let counts = changed.entry(class(path).to_owned()).or_insert((0, 0));
            counts.0 += added;
            counts.1 += removed;
        }
    }
    Ok(changed)
}

fn density(metrics: &Metrics) -> f64 {
    let lines = metrics.lines.get("production").copied().unwrap_or(0);
    let comments = metrics.comments.get("production").copied().unwrap_or(0);
    if lines == 0 {
        0.0
    } else {
        comments as f64 * 100.0 / lines as f64
    }
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
        let (added, removed) = changed.get(category).copied().unwrap_or_default();
        out.push_str(&format!(
            "| Lines changed: {category} | | +{added} / −{removed} | |\n"
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
    out.push_str(&format!(
        "| Comment density in production code (%) | {:.1} | {:.1} | |\n",
        density(&base),
        density(&head)
    ));
    let before = base.lines.get("production").copied().unwrap_or(0);
    let after = head.lines.get("production").copied().unwrap_or(0);
    out.push_str(&format!(
        "| Production lines | {before} | {after} ({}) | |\n",
        signed_delta(before, after)
    ));
    Ok(out)
}
