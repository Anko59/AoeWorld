//! Codex usage from the rate-limit snapshots its sessions log
//! (`~/.codex/sessions/**/rollout-*.jsonl`, `token_count` events). The newest
//! snapshot is the last thing the service told the client.
use super::iso;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

const NEWEST_FILES: usize = 6;
const TAIL_BYTES: u64 = 4 << 20;

/// One window of a snapshot.
#[derive(Debug, PartialEq)]
pub(super) struct Window {
    pub(super) used_percent: f64,
    pub(super) minutes: u64,
    pub(super) resets_at: u64,
}

#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    pub(super) timestamp: String,
    pub(super) plan: String,
    pub(super) windows: Vec<Window>,
}

fn window(value: &Value) -> Option<Window> {
    Some(Window {
        used_percent: value["used_percent"].as_f64()?,
        minutes: value["window_minutes"].as_u64()?,
        resets_at: value["resets_at"].as_u64()?,
    })
}

/// The newest snapshot among rollout lines, if any carries a window.
pub(super) fn latest(lines: impl Iterator<Item = String>) -> Option<Snapshot> {
    let mut best: Option<Snapshot> = None;
    for line in lines {
        if !line.contains("\"rate_limits\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let limits = &value["payload"]["rate_limits"];
        let windows: Vec<Window> = ["primary", "secondary"]
            .iter()
            .filter_map(|k| window(&limits[*k]))
            .collect();
        let Some(timestamp) = value["timestamp"].as_str() else {
            continue;
        };
        if windows.is_empty()
            || best
                .as_ref()
                .is_some_and(|b| b.timestamp.as_str() >= timestamp)
        {
            continue;
        }
        best = Some(Snapshot {
            timestamp: timestamp.to_owned(),
            plan: limits["plan_type"]
                .as_str()
                .unwrap_or("unknown plan")
                .to_owned(),
            windows,
        });
    }
    best
}

pub(super) fn render(snapshot: Option<Snapshot>, now: u64) -> String {
    let Some(snapshot) = snapshot else {
        return "Codex\n  unknown: no session under ~/.codex/sessions logged a rate-limit snapshot\n".to_owned();
    };
    let mut out = format!(
        "Codex (plan {}, last service snapshot {})\n",
        snapshot.plan, snapshot.timestamp
    );
    for w in &snapshot.windows {
        if w.resets_at <= now {
            out.push_str(&format!(
                "  {} min window: unknown: that window reset at {} and no later snapshot exists\n",
                w.minutes,
                iso(w.resets_at)
            ));
        } else {
            out.push_str(&format!(
                "  {} min window: {:.1}% used, {:.1}% left, resets {}\n",
                w.minutes,
                w.used_percent,
                100.0 - w.used_percent,
                iso(w.resets_at)
            ));
        }
    }
    out
}

fn rollouts(dir: &Path, into: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            rollouts(&path, into);
        } else if let Ok(modified) = meta.modified() {
            into.push((modified, path));
        }
    }
}

fn tail_lines(path: &Path) -> Vec<String> {
    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let size = file.metadata().map_or(0, |m| m.len());
    let start = size.saturating_sub(TAIL_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    // A tail starts mid-line unless it is the whole file.
    let skip = usize::from(start > 0);
    text.lines().skip(skip).map(str::to_owned).collect()
}

pub(super) fn section(sessions: &Path, now: u64) -> String {
    if !sessions.is_dir() {
        return format!("Codex\n  unknown: {} does not exist\n", sessions.display());
    }
    let mut files = Vec::new();
    rollouts(sessions, &mut files);
    files.sort_by_key(|file| std::cmp::Reverse(file.0));
    let lines = files
        .into_iter()
        .take(NEWEST_FILES)
        .flat_map(|(_, path)| tail_lines(&path));
    render(latest(lines), now)
}
