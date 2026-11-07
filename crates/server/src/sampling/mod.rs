//! Opt-in in-process sampling profiler (`--features sampling-profiler`).
//!
//! `scale-match --profile <dir>` builds the release server with the feature
//! and sets `AOE_SAMPLING_PROFILE`; after a delay the server samples every
//! thread (SIGPROF, `pprof`) for a window and writes into the directory:
//! `stacks.folded` (thread;outer;...;leaf count), `flame.svg` and `top.txt`
//! (the frames with most self and inclusive samples). Diagnostics only: the
//! simulation never sees it and a normal build does not compile it.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
#[cfg(feature = "sampling-profiler")]
use std::{path::Path, time::Duration};

/// Samples per second of process CPU time (`ITIMER_PROF`; a prime, so it
/// does not beat with the tick).
const FREQUENCY: i32 = 499;
/// Rows of each table in `top.txt`.
const TOP: usize = 60;

#[cfg(feature = "sampling-profiler")]
pub fn spawn(profile: crate::SamplingProfile) {
    let spawned = std::thread::Builder::new()
        .name("sampling-profiler".to_owned())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(profile.after_seconds));
            if let Err(error) = record(&profile) {
                tracing::error!(%error, "sampling profile failed");
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the sampling profiler");
    }
}

#[cfg(feature = "sampling-profiler")]
fn record(profile: &crate::SamplingProfile) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(seconds = profile.seconds, "sampling profile started");
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(FREQUENCY)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()?;
    std::thread::sleep(Duration::from_secs(profile.seconds));
    let report = guard.report().build()?;
    drop(guard);
    let mut stacks: BTreeMap<Vec<String>, u64> = BTreeMap::new();
    for (frames, count) in &report.data {
        let mut stack = vec![frames.thread_name.clone()];
        for frame in frames.frames.iter().rev() {
            for symbol in frame.iter().rev() {
                stack.push(short_name(&symbol.name()));
            }
        }
        *stacks.entry(stack).or_default() += u64::try_from(*count).unwrap_or(0);
    }
    std::fs::create_dir_all(&profile.directory)?;
    write_outputs(&profile.directory, &stacks)?;
    tracing::info!(directory = %profile.directory.display(), "sampling profile written");
    Ok(())
}

/// Drops the hash suffix and generic noise that make frames unreadable.
fn short_name(name: &str) -> String {
    let name = match name.rfind("::h") {
        Some(at) if name[at + 3..].chars().all(|c| c.is_ascii_hexdigit()) => &name[..at],
        _ => name,
    };
    name.replace(';', ":")
}

#[cfg(feature = "sampling-profiler")]
fn write_outputs(directory: &Path, stacks: &BTreeMap<Vec<String>, u64>) -> std::io::Result<()> {
    let mut folded = String::new();
    for (stack, count) in stacks {
        let _ = writeln!(folded, "{} {count}", stack.join(";"));
    }
    std::fs::write(directory.join("stacks.folded"), folded)?;
    std::fs::write(directory.join("top.txt"), top_table(stacks))?;
    std::fs::write(directory.join("flame.svg"), flame_svg(stacks))
}

/// Self (leaf) and inclusive (anywhere on the stack, once per stack) counts.
fn top_table(stacks: &BTreeMap<Vec<String>, u64>) -> String {
    let total: u64 = stacks.values().sum();
    let mut own: BTreeMap<&str, u64> = BTreeMap::new();
    let mut inclusive: BTreeMap<&str, u64> = BTreeMap::new();
    let mut threads: BTreeMap<&str, u64> = BTreeMap::new();
    for (stack, count) in stacks {
        if let Some(leaf) = stack.last().filter(|_| stack.len() > 1) {
            *own.entry(leaf).or_default() += count;
        }
        *threads.entry(&stack[0]).or_default() += count;
        let unique: BTreeSet<&str> = stack[1..].iter().map(String::as_str).collect();
        for frame in unique {
            *inclusive.entry(frame).or_default() += count;
        }
    }
    let mut out = format!("samples {total} at {FREQUENCY} Hz of process CPU time\n");
    for (title, table) in [
        ("threads", &threads),
        ("self", &own),
        ("inclusive", &inclusive),
    ] {
        let mut rows: Vec<_> = table.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let _ = writeln!(out, "\n== {title} ==");
        for (name, count) in rows.into_iter().take(TOP) {
            let share = *count as f64 * 100.0 / total.max(1) as f64;
            let _ = writeln!(out, "{share:6.2}% {count:8} {name}");
        }
    }
    out
}

#[derive(Default)]
struct Node {
    count: u64,
    children: BTreeMap<String, Node>,
}

const ROW: f64 = 16.0;
const WIDTH: f64 = 1800.0;

/// A plain icicle-ordered flame graph (root at the bottom), hover titles.
fn flame_svg(stacks: &BTreeMap<Vec<String>, u64>) -> String {
    let mut root = Node::default();
    let mut depth = 0;
    for (stack, count) in stacks {
        root.count += count;
        let mut node = &mut root;
        for frame in stack {
            node = node.children.entry(frame.clone()).or_default();
            node.count += count;
        }
        depth = depth.max(stack.len());
    }
    let height = (depth as f64 + 2.0) * ROW;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" \
         font-family=\"monospace\" font-size=\"11\"><rect width=\"100%\" height=\"100%\" \
         fill=\"#fdfdf6\"/>\n"
    );
    let scale = WIDTH / root.count.max(1) as f64;
    let mut x = 0.0;
    for (name, child) in &root.children {
        draw(&mut svg, name, child, x, 0, height, scale, root.count);
        x += child.count as f64 * scale;
    }
    svg.push_str("</svg>\n");
    svg
}

#[allow(clippy::too_many_arguments)]
fn draw(
    svg: &mut String,
    name: &str,
    node: &Node,
    x: f64,
    level: usize,
    height: f64,
    scale: f64,
    total: u64,
) {
    let width = node.count as f64 * scale;
    if width < 0.3 {
        return;
    }
    let y = height - (level as f64 + 2.0) * ROW;
    let hash = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let (r, g) = (205 + hash % 50, 80 + (hash >> 8) % 120);
    let escaped = name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let share = node.count as f64 * 100.0 / total.max(1) as f64;
    let _ = write!(
        svg,
        "<g><title>{escaped} ({} samples, {share:.2}%)</title><rect x=\"{x:.1}\" y=\"{y:.1}\" \
         width=\"{width:.1}\" height=\"{}\" fill=\"rgb({r},{g},60)\" stroke=\"#fff\" stroke-width=\"0.3\"/>",
        node.count,
        ROW - 1.0
    );
    let chars = (width / 6.8) as usize;
    if chars >= 4 {
        let label: String = escaped.chars().take(chars.saturating_sub(1)).collect();
        let _ = write!(
            svg,
            "<text x=\"{:.1}\" y=\"{:.1}\">{label}</text>",
            x + 2.0,
            y + ROW - 4.0
        );
    }
    svg.push_str("</g>\n");
    let mut child_x = x;
    for (child_name, child) in &node.children {
        draw(
            svg,
            child_name,
            child,
            child_x,
            level + 1,
            height,
            scale,
            total,
        );
        child_x += child.count as f64 * scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_and_graph_count_self_and_inclusive_samples() {
        let mut stacks = BTreeMap::new();
        stacks.insert(vec!["tick".into(), "a".into(), "b".into()], 3);
        stacks.insert(vec!["tick".into(), "a".into()], 1);
        stacks.insert(vec!["tick".into(), "b".into(), "b".into()], 2);
        let top = top_table(&stacks);
        assert!(top.contains("samples 6"));
        assert!(top.contains("83.33%        5 b"), "{top}");
        assert!(top.contains("66.67%        4 a"), "{top}");
        let svg = flame_svg(&stacks);
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("<title>a (4 samples"));
        assert_eq!(short_name("x::y::h0123456789abcdef"), "x::y");
    }
}
