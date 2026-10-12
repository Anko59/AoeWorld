//! The pull request metrics table: one row per metric, base against PR, with
//! the rows chosen by the PR level (docs/shipping.md).
use super::{LineCounts, snapshot::Snapshot};
use crate::ship::describe::Level;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
enum Good {
    Up,
    Down,
    Neutral,
}

#[derive(Clone, Copy)]
enum Kind {
    /// `after (delta)`, with `(—)` when a side is absent.
    Count,
    /// One decimal; the delta only when both sides are present.
    Decimal,
}

struct Row {
    label: String,
    before: Option<f64>,
    after: Option<f64>,
    kind: Kind,
    good: Good,
    level: Level,
    /// Measured rows with no value on either side are left out.
    measured: bool,
}

fn number(value: f64, kind: Kind) -> String {
    match kind {
        Kind::Count => format!("{value:.0}"),
        Kind::Decimal => format!("{value:.1}"),
    }
}

fn signed(before: f64, after: f64, kind: Kind) -> String {
    let delta = after - before;
    let text = number(delta.abs(), kind);
    if delta >= 0.0 {
        format!("+{text}")
    } else {
        format!("−{text}")
    }
}

impl Row {
    fn text(&self) -> String {
        let show =
            |value: Option<f64>| value.map_or_else(|| "—".to_owned(), |v| number(v, self.kind));
        let delta = match (self.before, self.after, self.kind) {
            (Some(b), Some(a), _) => Some(signed(b, a, self.kind)),
            (_, _, Kind::Count) => Some("—".to_owned()),
            _ => None,
        };
        let after = match delta {
            Some(delta) => format!("{} ({delta})", show(self.after)),
            None => show(self.after),
        };
        let marker = match (self.before, self.after, self.good) {
            (Some(b), Some(a), good)
                if good != Good::Neutral && number(b, self.kind) != number(a, self.kind) =>
            {
                if (a > b) == (good == Good::Up) {
                    " 🟢"
                } else {
                    " 🔴"
                }
            }
            _ => "",
        };
        format!(
            "| {} | {} | {after} |{marker} |\n",
            self.label,
            show(self.before)
        )
    }
}

fn count(map: &BTreeMap<String, u64>, key: &str) -> Option<f64> {
    map.get(key).map(|value| *value as f64)
}

fn density(snapshot: &Snapshot, class: &str) -> Option<f64> {
    let lines = *snapshot.statics.lines.get(class)?;
    let comments = snapshot.statics.comments.get(class).copied().unwrap_or(0);
    (lines > 0).then(|| comments as f64 * 100.0 / lines as f64)
}

fn rows(before: &Snapshot, after: &Snapshot) -> Vec<Row> {
    let (b, a) = (&before.statics, &after.statics);
    let mut out = Vec::new();
    let mut row = |label: &str, pair: (Option<f64>, Option<f64>), kind, good, level, measured| {
        out.push(Row {
            label: label.to_owned(),
            before: pair.0,
            after: pair.1,
            kind,
            good,
            level,
            measured,
        });
    };
    use {Good::*, Kind::*, Level::*};
    for layer in ["unit", "integration"] {
        let tests = |m: &BTreeMap<String, u64>| Some(m.get(layer).copied().unwrap_or(0) as f64);
        row(
            &format!("Tests: {layer} (source #[test] attributes)"),
            (tests(&b.tests), tests(&a.tests)),
            Count,
            Up,
            Low,
            false,
        );
    }
    row(
        "Comment density in production code (%)",
        (density(before, "production"), density(after, "production")),
        Decimal,
        Up,
        Low,
        false,
    );
    row(
        "Production lines",
        (count(&b.lines, "production"), count(&a.lines, "production")),
        Count,
        Neutral,
        Low,
        false,
    );
    let largest = |m: &super::Metrics| m.largest_file.as_ref().map(|f| f.lines as f64);
    row(
        "Largest human-authored file (lines, limit 500)",
        (largest(b), largest(a)),
        Count,
        Down,
        Low,
        false,
    );
    for (label, key) in [
        ("wasm", "wasm"),
        ("Playwright", "playwright"),
        ("fuzz targets", "fuzz"),
    ] {
        row(
            &format!("Tests: {label}"),
            (count(&b.tests, key), count(&a.tests, key)),
            Count,
            Up,
            Medium,
            false,
        );
    }
    let sum = |m: &super::Metrics, keys: &[&str]| {
        Some(
            keys.iter()
                .filter_map(|k| m.dependencies.get(*k))
                .sum::<u64>() as f64,
        )
    };
    row(
        "Dependencies: Cargo (third-party packages)",
        (sum(b, &["cargo", "fuzz"]), sum(a, &["cargo", "fuzz"])),
        Count,
        Down,
        Medium,
        false,
    );
    row(
        "Dependencies: npm (packages)",
        (sum(b, &["npm"]), sum(a, &["npm"])),
        Count,
        Down,
        Medium,
        false,
    );
    row(
        "`unsafe` lines in production code",
        (Some(b.unsafe_count as f64), Some(a.unsafe_count as f64)),
        Count,
        Down,
        Medium,
        false,
    );
    row(
        "TODO/FIXME lines",
        (Some(b.todo_count as f64), Some(a.todo_count as f64)),
        Count,
        Down,
        Medium,
        false,
    );
    row(
        "Files within 10% of the 500-line limit",
        (
            Some(b.files_near_limit as f64),
            Some(a.files_near_limit as f64),
        ),
        Count,
        Down,
        Medium,
        false,
    );
    row(
        "Files over the 500-line limit",
        (
            Some(b.files_over_limit.len() as f64),
            Some(a.files_over_limit.len() as f64),
        ),
        Count,
        Down,
        Medium,
        false,
    );
    for class in ["tests", "harness", "docs", "config", "generated"] {
        row(
            &format!("Lines total: {class}"),
            (count(&b.lines, class), count(&a.lines, class)),
            Count,
            Neutral,
            High,
            false,
        );
    }
    for class in ["tests", "harness"] {
        row(
            &format!("Comment density in {class} code (%)"),
            (density(before, class), density(after, class)),
            Decimal,
            Neutral,
            High,
            false,
        );
    }
    let (mb, ma) = (&before.measured, &after.measured);
    let int = |v: Option<u64>| v.map(|v| v as f64);
    row(
        "Gate time of `make ship` (s)",
        (mb.gate_seconds, ma.gate_seconds),
        Decimal,
        Down,
        High,
        true,
    );
    row(
        "WASM size (bytes)",
        (int(mb.wasm_bytes), int(ma.wasm_bytes)),
        Count,
        Down,
        High,
        true,
    );
    row(
        "WASM size gzip (bytes)",
        (int(mb.wasm_gzip_bytes), int(ma.wasm_gzip_bytes)),
        Count,
        Down,
        High,
        true,
    );
    row(
        "Native server binary (bytes)",
        (int(mb.native_bytes), int(ma.native_bytes)),
        Count,
        Down,
        High,
        true,
    );
    row(
        "Line coverage (%)",
        (mb.coverage_percent, ma.coverage_percent),
        Decimal,
        Up,
        Max,
        true,
    );
    let perf = |m: &super::measured::Measured, pick: fn(&super::measured::Perf) -> u64| {
        m.perf.as_ref().map(|p| pick(p) as f64)
    };
    row(
        "Perf smoke: p95 tick (ns)",
        (perf(mb, |p| p.p95_tick_ns), perf(ma, |p| p.p95_tick_ns)),
        Count,
        Down,
        Max,
        true,
    );
    row(
        "Perf smoke: entities",
        (perf(mb, |p| p.entities), perf(ma, |p| p.entities)),
        Count,
        Neutral,
        Max,
        true,
    );
    row(
        "Perf smoke: encoded bytes",
        (perf(mb, |p| p.bytes), perf(ma, |p| p.bytes)),
        Count,
        Down,
        Max,
        true,
    );
    out
}

pub(super) fn render(
    before: &Snapshot,
    after: &Snapshot,
    changed: &BTreeMap<String, LineCounts>,
    level: Level,
) -> Result<String, String> {
    let mut out = String::from("| 📊 Metric | merge base | this PR | |\n|---|---:|---:|---:|\n");
    let shown = if level >= Level::Medium { 6 } else { 5 };
    for category in [
        "production",
        "tests",
        "harness",
        "docs",
        "config",
        "generated",
    ]
    .iter()
    .take(shown)
    {
        let (added, removed) = changed
            .get(*category)
            .copied()
            .unwrap_or((Some(0), Some(0)));
        let show = |v: Option<u64>| v.map_or_else(|| "—".to_owned(), |v| v.to_string());
        out.push_str(&format!(
            "| Lines changed: {category} | | +{} / −{} | |\n",
            show(added),
            show(removed)
        ));
    }
    for row in rows(before, after) {
        if row.level > level || (row.measured && row.before.is_none() && row.after.is_none()) {
            continue;
        }
        out.push_str(&row.text());
    }
    Ok(out)
}
