//! Static metrics of one commit, read from its git blobs without a build.
use super::{class, scan_rust};
use crate::policy::is_text;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_LOCK_BYTES: usize = 16 * 1024 * 1024;
const LINE_LIMIT: u64 = 500;
const NEAR_LIMIT: u64 = LINE_LIMIT * 9 / 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::ship) struct LargestFile {
    pub(in crate::ship) path: String,
    pub(in crate::ship) lines: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::ship) struct Metrics {
    /// Lines by [`class`].
    pub(in crate::ship) lines: BTreeMap<String, u64>,
    /// Comment lines by class (Rust lexer; `//` or `#` lines elsewhere).
    pub(in crate::ship) comments: BTreeMap<String, u64>,
    /// `unit`, `integration`, `wasm`, `playwright` and `fuzz` test counts.
    pub(in crate::ship) tests: BTreeMap<String, u64>,
    /// Third-party packages: `cargo`, `fuzz` (lockfiles) and `npm`.
    pub(in crate::ship) dependencies: BTreeMap<String, u64>,
    /// Production lines that mention the `unsafe` keyword.
    pub(in crate::ship) unsafe_count: u64,
    /// Production, test and config lines with `TODO` or `FIXME`.
    pub(in crate::ship) todo_count: u64,
    /// The longest human-authored text file.
    pub(in crate::ship) largest_file: Option<LargestFile>,
    /// Human-authored files of 450 to 500 lines.
    pub(in crate::ship) files_near_limit: u64,
    /// Human-authored files over 500 lines.
    pub(in crate::ship) files_over_limit: Vec<String>,
}

impl Metrics {
    pub(in crate::ship) fn test_total(&self) -> u64 {
        self.tests.values().sum()
    }

    pub(in crate::ship) fn dependency_total(&self) -> u64 {
        self.dependencies.values().sum()
    }
}

fn lock_file(path: &str) -> bool {
    matches!(
        path,
        "Cargo.lock" | "fuzz/Cargo.lock" | "browser/package-lock.json"
    )
}

fn wanted(path: &str) -> bool {
    lock_file(path) || (is_text(Path::new(path)) && !path.ends_with("-lock.json"))
}

fn source_paths(root: &Path, revision: &str) -> Result<Vec<(String, String)>, String> {
    let listing = super::git::git(root, &["ls-tree", "-rz", "--full-tree", revision])?;
    Ok(listing
        .split('\0')
        .filter_map(|entry| {
            let (metadata, path) = entry.split_once('\t')?;
            let mut fields = metadata.split_whitespace();
            let mode = fields.next()?;
            let kind = fields.next()?;
            let oid = fields.next()?;
            (kind == "blob" && mode != "120000" && wanted(path))
                .then(|| (path.to_owned(), oid.to_owned()))
        })
        .collect())
}

/// Snapshot the tracked text of a commit or merge-base revision. Blobs over
/// 1 MiB (16 MiB for lockfiles) and binary blobs are omitted.
pub(in crate::ship) fn repository(root: &Path, revision: &str) -> Result<Metrics, String> {
    let objects = source_paths(root, revision)?;
    blobs(root, objects)
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
        let cap = if lock_file(path) {
            MAX_LOCK_BYTES
        } else {
            MAX_SOURCE_BYTES
        };
        let keep = size <= cap;
        let mut bytes = Vec::new();
        let wanted = if keep { size } else { 0 } as u64;
        (&mut output)
            .take(wanted)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("reading blob {path}: {error}"))?;
        if !keep {
            io::copy(&mut (&mut output).take(size as u64), &mut io::sink())
                .map_err(|error| format!("draining large blob {path}: {error}"))?;
        }
        let mut delimiter = [0];
        output
            .read_exact(&mut delimiter)
            .map_err(|error| format!("reading blob delimiter {path}: {error}"))?;
        if delimiter[0] != b'\n' {
            return Err(format!("invalid cat-file delimiter for {path}"));
        }
        if keep
            && !bytes.contains(&0)
            && let Ok(source) = std::str::from_utf8(&bytes)
        {
            accumulate(&mut metrics, path, source);
        }
    }
    Ok(metrics)
}

fn add(map: &mut BTreeMap<String, u64>, key: &str, value: u64) {
    *map.entry(key.to_owned()).or_default() += value;
}

fn words(line: &str, word: &str) -> bool {
    let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    line.match_indices(word).any(|(at, _)| {
        boundary(line[..at].chars().next_back()) && boundary(line[at + word.len()..].chars().next())
    })
}

fn newline_lines(source: &str) -> u64 {
    source.bytes().filter(|b| *b == b'\n').count() as u64
        + u64::from(!source.is_empty() && !source.ends_with('\n'))
}

/// Lines starting with the language's line-comment marker.
fn line_comments(path: &str, source: &str) -> u64 {
    let name = path.rsplit('/').next().unwrap_or(path);
    let marker = match name.rsplit_once('.').map(|(_, ext)| ext) {
        Some("ts" | "tsx" | "js" | "mjs" | "wgsl") => "//",
        Some("sh" | "yml" | "yaml" | "toml" | "mk" | "Dockerfile") => "#",
        _ if matches!(name, "Makefile" | "Dockerfile") => "#",
        _ => return 0,
    };
    source
        .lines()
        .filter(|line| line.trim_start().starts_with(marker))
        .count() as u64
}

fn dependencies(path: &str, source: &str) -> Option<(&'static str, u64)> {
    match path {
        "Cargo.lock" | "fuzz/Cargo.lock" => {
            // Third-party packages have a registry or git `source`.
            let count = source
                .split("[[package]]")
                .skip(1)
                .filter(|block| block.lines().any(|line| line.starts_with("source = ")))
                .count();
            Some((
                if path == "Cargo.lock" {
                    "cargo"
                } else {
                    "fuzz"
                },
                count as u64,
            ))
        }
        "browser/package-lock.json" => {
            let value: serde_json::Value = serde_json::from_str(source).ok()?;
            let packages = value.get("packages")?.as_object()?;
            Some((
                "npm",
                packages.keys().filter(|key| !key.is_empty()).count() as u64,
            ))
        }
        _ => None,
    }
}

fn accumulate(metrics: &mut Metrics, path: &str, source: &str) {
    let category = class(path);
    if let Some((kind, count)) = dependencies(path, source) {
        add(&mut metrics.dependencies, kind, count);
    }
    let rust = path.ends_with(".rs");
    let (lines, comments, attributes) = if rust {
        scan_rust(source)
    } else {
        (newline_lines(source), line_comments(path, source), 0)
    };
    add(&mut metrics.lines, category, lines);
    if path.ends_with(".lock") || path.ends_with("-lock.json") {
        return;
    }
    add(&mut metrics.comments, category, comments);
    if rust {
        let integration = path.starts_with("crates/") && path.split('/').nth(2) == Some("tests");
        add(
            &mut metrics.tests,
            if integration { "integration" } else { "unit" },
            attributes,
        );
        let wasm = source
            .lines()
            .filter(|line| line.trim_start().starts_with("#[wasm_bindgen_test"))
            .count();
        add(&mut metrics.tests, "wasm", wasm as u64);
        if path.starts_with("fuzz/fuzz_targets/") {
            add(&mut metrics.tests, "fuzz", 1);
        }
        if category == "production" {
            let uses = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//") && words(line, "unsafe"));
            metrics.unsafe_count += uses.count() as u64;
        }
    } else if path.starts_with("browser/tests/")
        && matches!(path.rsplit('.').next(), Some("ts" | "js" | "mjs"))
    {
        let cases = source
            .lines()
            .filter(|line| line.trim_start().starts_with("test("));
        add(&mut metrics.tests, "playwright", cases.count() as u64);
    }
    if matches!(category, "production" | "tests" | "config") {
        let marked = source
            .lines()
            .filter(|line| line.contains("TODO") || line.contains("FIXME"));
        metrics.todo_count += marked.count() as u64;
    }
    if lines > LINE_LIMIT {
        metrics.files_over_limit.push(path.to_owned());
    } else if lines >= NEAR_LIMIT {
        metrics.files_near_limit += 1;
    }
    if metrics
        .largest_file
        .as_ref()
        .is_none_or(|top| lines > top.lines)
    {
        metrics.largest_file = Some(LargestFile {
            path: path.to_owned(),
            lines,
        });
    }
}
