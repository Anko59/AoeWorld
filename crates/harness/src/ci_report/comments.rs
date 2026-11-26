//! One CI performance comment, read back into numbers. The comment is the
//! format `perf_comment::render` writes (old and new wording share the
//! tables): a hidden marker, a sentence naming the measured revision and the
//! `dev` revision it was compared with, a `Metric | dev | this PR` table and a
//! `CI job | Duration` table.
use std::collections::BTreeMap;

pub(super) const MARKER: &str = "<!-- aoe-perf-comment -->";

/// What the comment says, with the comment's last edit as its date.
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub(super) pr: u64,
    pub(super) at: String,
    pub(super) run: Option<u64>,
    pub(super) dev: Option<DevSide>,
    pub(super) jobs: Vec<JobTime>,
}

/// The `dev` column: the compared commit and its counts by row. A row is
/// `simulation::late_game` for `Instructions: simulation::late_game`, and
/// `wasm_gzip` or `wasm_raw` for the bundle sizes. Timing rows are left out.
#[derive(Clone, Debug)]
pub(super) struct DevSide {
    pub(super) rev: String,
    pub(super) rows: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct JobTime {
    pub(super) name: String,
    pub(super) seconds: u64,
    pub(super) failed: bool,
}

/// `1h02m`, `16m00s` or `47s` in seconds.
pub(super) fn parse_duration(text: &str) -> Option<u64> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut total = 0;
    for (unit, factor) in [('h', 3_600), ('m', 60), ('s', 1)] {
        if let Some((number, tail)) = rest.split_once(unit)
            && !number.is_empty()
            && number.bytes().all(|b| b.is_ascii_digit())
        {
            total += number.parse::<u64>().ok()? * factor;
            rest = tail;
        }
    }
    rest.is_empty().then_some(total)
}

fn revision_after<'a>(body: &'a str, marker: &str) -> Option<&'a str> {
    let start = body.find(marker)? + marker.len();
    let rev = body.get(start..start + 40)?;
    (rev.bytes().all(|b| b.is_ascii_hexdigit()) && body[start + 40..].starts_with('`'))
        .then_some(rev)
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

/// The table that follows `header` (a line starting with it), as rows of cells.
fn table<'a>(body: &'a str, header: &str) -> Option<(Vec<&'a str>, Vec<Vec<&'a str>>)> {
    let mut lines = body.lines().skip_while(|line| !line.starts_with(header));
    let head = cells(lines.next()?);
    let rows = lines
        .skip(1)
        .take_while(|line| line.starts_with('|'))
        .map(cells)
        .collect();
    Some((head, rows))
}

fn row_name(metric: &str) -> Option<String> {
    match metric {
        "Optimized WASM raw (bytes)" => Some("wasm_raw".to_owned()),
        "Optimized WASM gzip (bytes)" => Some("wasm_gzip".to_owned()),
        _ => metric.strip_prefix("Instructions: ").map(str::to_owned),
    }
}

fn dev_rows(body: &str) -> BTreeMap<String, u64> {
    let mut rows = BTreeMap::new();
    let Some((head, table)) = table(body, "| Metric |") else {
        return rows;
    };
    let Some(column) = head.iter().position(|name| *name == "dev") else {
        return rows;
    };
    for row in table {
        if let (Some(name), Some(value)) = (
            row.first().and_then(|metric| row_name(metric)),
            row.get(column).and_then(|cell| cell.parse::<u64>().ok()),
        ) {
            rows.insert(name, value);
        }
    }
    rows
}

/// A job's status is appended to its name when it did not succeed.
const STATUSES: [&str; 6] = [
    "failure",
    "timed_out",
    "cancelled",
    "action_required",
    "startup_failure",
    "neutral",
];

fn job_time(row: &[&str]) -> Option<JobTime> {
    let seconds = parse_duration(row.get(1)?)?;
    let name = *row.first()?;
    if let Some((base, rest)) = name.rsplit_once(" (")
        && let Some(status) = rest.strip_suffix(')')
        && STATUSES.contains(&status)
    {
        // A cancelled job stopped for another reason: it says nothing about its own time.
        return matches!(status, "failure" | "timed_out" | "startup_failure").then(|| JobTime {
            name: base.to_owned(),
            seconds,
            failed: true,
        });
    }
    Some(JobTime {
        name: name.to_owned(),
        seconds,
        failed: false,
    })
}

/// The numbers of one comment, or `None` when the body is not the CI bot's
/// performance comment (it must start with the marker).
pub(super) fn parse_comment(pr: u64, at: &str, body: &str) -> Option<Observation> {
    if !body.starts_with(MARKER) {
        return None;
    }
    let measured = revision_after(body, "measured revision `").map(str::to_owned);
    // The dev revision is named after the measured one: "(revision `…`)".
    let compared = measured
        .as_ref()
        .and_then(|_| body.split_once("measured revision `").map(|(_, tail)| tail))
        .and_then(|tail| revision_after(tail, "(revision `"));
    let rows = dev_rows(body);
    let dev = compared.filter(|_| !rows.is_empty()).map(|rev| DevSide {
        rev: rev.to_owned(),
        rows,
    });
    let run = body
        .split_once("Job durations of run ")
        .or_else(|| body.split_once("actions/runs/"))
        .and_then(|(_, tail)| {
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        });
    let jobs = table(body, "| CI job |")
        .map(|(_, rows)| rows.iter().filter_map(|row| job_time(row)).collect())
        .unwrap_or_default();
    Some(Observation {
        pr,
        at: at.to_owned(),
        run,
        dev,
        jobs,
    })
}
