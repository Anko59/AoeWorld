//! Claude Code usage from its local transcripts (`~/.claude/projects`): the
//! tokens each model consumed in the window. The plan's remaining quota is
//! held by Anthropic's service and shown only inside the interactive client,
//! so that part is reported as unknown.
use super::iso;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Tokens {
    pub(super) messages: u64,
    pub(super) input: u64,
    pub(super) output: u64,
    pub(super) cache_write: u64,
    pub(super) cache_read: u64,
}

/// Add the assistant messages of one transcript dated at or after `since`
/// (an ISO timestamp). A message appears once however often it was logged.
pub(super) fn tally(
    lines: impl Iterator<Item = String>,
    since: &str,
    totals: &mut BTreeMap<String, Tokens>,
    seen: &mut HashSet<String>,
) {
    for line in lines {
        if !line.contains("\"usage\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let message = &value["message"];
        let (Some(model), Some(usage)) = (message["model"].as_str(), message.get("usage")) else {
            continue;
        };
        if model.starts_with('<') || value["timestamp"].as_str().is_none_or(|t| t < since) {
            continue;
        }
        if let Some(id) = message["id"].as_str()
            && !seen.insert(id.to_owned())
        {
            continue;
        }
        let count = |key: &str| usage[key].as_u64().unwrap_or(0);
        let row = totals.entry(model.to_owned()).or_default();
        row.messages += 1;
        row.input += count("input_tokens");
        row.output += count("output_tokens");
        row.cache_write += count("cache_creation_input_tokens");
        row.cache_read += count("cache_read_input_tokens");
    }
}

fn transcripts(dir: &Path, newer_than: SystemTime, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            transcripts(&path, newer_than, into);
        } else if path.extension().is_some_and(|e| e == "jsonl")
            && meta.modified().is_ok_and(|m| m >= newer_than)
        {
            into.push(path);
        }
    }
}

fn human(n: u64) -> String {
    match n {
        1_000_000.. => format!("{:.1}M", n as f64 / 1e6),
        1_000.. => format!("{:.1}k", n as f64 / 1e3),
        _ => n.to_string(),
    }
}

pub(super) fn render(totals: &BTreeMap<String, Tokens>, files: usize) -> String {
    let mut out =
        format!("Claude Code, tokens used in the last 7 days ({files} local transcripts)\n");
    for (model, t) in totals {
        out.push_str(&format!(
            "  {model:<24} {:>6} replies  in {:>7}  out {:>7}  cache write {:>7}  cache read {:>7}\n",
            t.messages,
            human(t.input),
            human(t.output),
            human(t.cache_write),
            human(t.cache_read)
        ));
    }
    if totals.is_empty() {
        out.push_str("  none found\n");
    }
    out.push_str("  remaining: unknown: the plan's quota is held by Anthropic and shown only by /usage in the interactive client\n");
    out
}

pub(super) fn section(projects: &Path, since: u64) -> String {
    if !projects.is_dir() {
        return format!(
            "Claude Code\n  unknown: {} does not exist\n",
            projects.display()
        );
    }
    let newer_than = UNIX_EPOCH + Duration::from_secs(since);
    let mut files = Vec::new();
    transcripts(projects, newer_than, &mut files);
    let since_iso = iso(since);
    let (mut totals, mut seen) = (BTreeMap::new(), HashSet::new());
    for file in &files {
        if let Ok(f) = fs::File::open(file) {
            tally(
                BufReader::new(f).lines().map_while(Result::ok),
                &since_iso,
                &mut totals,
                &mut seen,
            );
        }
    }
    render(&totals, files.len())
}
