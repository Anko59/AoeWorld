//! The pull request section and the terminal text: the same tables, in Markdown under a hidden
//! marker for the comment `make ship` keeps current, in aligned plain text for the agent.
use super::{
    compare::{ScenarioDiff, Status, WindowDiff},
    numbers,
    verdict::{self, Verdicts},
};
use serde_json::{Value, json};

/// The first line of the comment: a re-ship finds its own comment by it, and `make ci-report`
/// can find every game score comment of the repository by it.
pub(crate) const MARKER: &str = "<!-- aoe-game-score -->";
/// Where `make ship` writes what the whole check added to the ship beyond the review: only ship
/// knows it, and only once the check has ended.
pub(crate) const ADDED: &str = "<!-- added-to-ship -->";
/// GitHub refuses a comment above 65,536 characters.
const COMMENT_LIMIT: usize = 60_000;

/// Where one side's numbers came from, and what that cost.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Source {
    /// An entry of the results cache.
    Cached {
        key: String,
        age_seconds: u64,
        revision: String,
        /// What producing the entry cost when it was made.
        play_seconds: f64,
        /// What finding it cost now.
        lookup_seconds: f64,
    },
    Played {
        build_seconds: f64,
        play_seconds: f64,
        key: Option<String>,
    },
    Unavailable(String),
}

impl Source {
    /// One line about this side. `miss`: a played result is a miss of the results cache (the
    /// merge base's), and building it is part of what the miss cost.
    fn line(&self, what: &str, miss: bool) -> String {
        match self {
            Source::Cached {
                key,
                age_seconds,
                revision,
                play_seconds,
                lookup_seconds,
            } => format!(
                "{what}: cache hit in {lookup_seconds:.1} s (entry {}, made from {} {} ago; it \
                 took {play_seconds:.0} s to play)",
                &key[..12.min(key.len())],
                &revision[..12.min(revision.len())],
                age(*age_seconds)
            ),
            Source::Played {
                build_seconds,
                play_seconds,
                key,
            } => format!(
                "{what}: {}{}played in {play_seconds:.0} s{}",
                if miss { "cache miss, " } else { "" },
                if *build_seconds > 0.0 {
                    format!("built in {build_seconds:.0} s, ")
                } else {
                    String::new()
                },
                key.as_deref().map_or(String::new(), |key| format!(
                    ", stored as entry {}",
                    &key[..12.min(key.len())]
                ))
            ),
            Source::Unavailable(why) => format!("{what}: not available ({why})"),
        }
    }
}

fn age(seconds: u64) -> String {
    match seconds {
        0..=119 => format!("{seconds} s"),
        120..=7199 => format!("{} min", seconds / 60),
        _ => format!("{} h", seconds / 3600),
    }
}

pub(super) struct Page<'a> {
    pub head: &'a str,
    pub base: Option<&'a str>,
    pub map_hash: &'a str,
    pub branch: Option<&'a Value>,
    pub base_report: Option<&'a Value>,
    pub diffs: &'a [ScenarioDiff],
    pub verdicts: &'a Verdicts,
    /// Things worth a line that no number says (an uncommitted tree, a result not stored).
    pub notes: &'a [String],
    pub branch_source: &'a Source,
    pub base_source: &'a Source,
}

struct Table {
    header: Vec<&'static str>,
    rows: Vec<Vec<String>>,
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\r', '\n'], " ")
}

impl Table {
    fn markdown(&self) -> String {
        let mut out = format!(
            "| {} |\n|{}\n",
            self.header.join(" | "),
            "---|".repeat(self.header.len())
        );
        for row in &self.rows {
            let cells: Vec<String> = row.iter().map(|text| cell(text)).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
        out
    }

    /// Aligned columns; the three last (numbers) to the right.
    fn text(&self) -> String {
        let columns = self.header.len();
        let width = |index: usize| {
            self.rows
                .iter()
                .map(|row| row[index].chars().count())
                .chain([self.header[index].chars().count()])
                .max()
                .unwrap_or(0)
        };
        let widths: Vec<usize> = (0..columns).map(width).collect();
        let line = |cells: Vec<&str>| {
            let parts: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    if index + 3 >= columns {
                        format!("{text:>width$}", width = widths[index])
                    } else {
                        format!("{text:<width$}", width = widths[index])
                    }
                })
                .collect();
            format!("  {}\n", parts.join("  ").trim_end())
        };
        let mut out = line(self.header.clone());
        for row in &self.rows {
            out.push_str(&line(row.iter().map(String::as_str).collect()));
        }
        out
    }
}

/// The rows of the numbers that changed: those the table names (`named` true), the others
/// (`named` false), or all of them (`None`).
fn changed_table(diff: &ScenarioDiff, named: Option<bool>) -> Table {
    let mut rows = Vec::new();
    for window in &diff.windows {
        for row in &window.changed {
            if named.is_some_and(|named| named != numbers::find(&row.path).is_some()) {
                continue;
            }
            // With nothing to compare with, the merge base's column is empty, not "none".
            let base = if diff.status == Status::Compared {
                numbers::show(&row.path, row.base)
            } else {
                "-".to_owned()
            };
            rows.push(vec![
                window.id.clone(),
                row.label(),
                base,
                numbers::show(&row.path, row.branch),
                verdict::change(row),
            ]);
        }
    }
    Table {
        header: vec!["window", "number", "merge base", "this branch", "change"],
        rows,
    }
}

fn noisy_label(path: &str) -> String {
    match path {
        "cost.tick_seconds" => "seconds inside the ticks".to_owned(),
        "cost.tick_ms.p95" => "tick p95 (ms)".to_owned(),
        "cost.peak_memory_mib" => "peak memory (MiB)".to_owned(),
        "cost.instructions" => "instructions".to_owned(),
        other => numbers::label(other),
    }
}

fn noisy_table(diff: &ScenarioDiff) -> Table {
    let mut rows = Vec::new();
    for window in &diff.windows {
        for row in &window.noisy {
            let change = row
                .percent()
                .map_or_else(|| "-".to_owned(), |percent| format!("{percent:+.0}%"));
            let show = |value: Option<f64>| {
                value.map_or("-".to_owned(), |v| {
                    if v.fract() == 0.0 {
                        format!("{v:.0}")
                    } else {
                        format!("{v:.1}")
                    }
                })
            };
            rows.push(vec![
                window.id.clone(),
                noisy_label(&row.path),
                show(row.base),
                show(row.branch),
                change,
            ]);
        }
    }
    Table {
        header: vec!["window", "cost", "merge base", "this branch", "change"],
        rows,
    }
}

fn unchanged_line(diff: &ScenarioDiff) -> String {
    let total: usize = diff.windows.iter().map(|window| window.unchanged).sum();
    let parts: Vec<String> = diff
        .windows
        .iter()
        .map(|window: &WindowDiff| format!("{} {}", window.id, window.unchanged))
        .collect();
    match diff.status {
        Status::Compared if diff.windows.iter().all(|w| w.changed.is_empty()) => format!(
            "no number changed: this branch plays the same game as the merge base ({total} numbers unchanged: {})",
            parts.join(", ")
        ),
        Status::Compared => format!("{total} numbers unchanged ({}).", parts.join(", ")),
        _ => String::new(),
    }
}

fn scenario_heading(diff: &ScenarioDiff, branch: Option<&Value>) -> String {
    let detail = branch
        .and_then(|report| {
            report["scenarios"]
                .as_array()?
                .iter()
                .find(|scenario| scenario["id"] == diff.id.as_str())
        })
        .map(|scenario| format!(", {} seats, {} ticks", scenario["seats"], scenario["ticks"]))
        .unwrap_or_default();
    format!("{}{detail}", diff.id)
}

fn timing_lines(page: &Page<'_>) -> Vec<String> {
    vec![
        page.base_source.line("merge base", true),
        page.branch_source.line("this branch", false),
    ]
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

fn title(page: &Page<'_>) -> String {
    format!(
        "merge base {} against this branch {} on map {}",
        page.base.map_or("(none)", short),
        short(page.head),
        &page.map_hash[..8.min(page.map_hash.len())]
    )
}

/// The text the agent reads in the terminal.
pub(super) fn terminal(page: &Page<'_>) -> String {
    let mut out = format!(
        "game score (informational, nothing is gated): {}\n\n",
        title(page)
    );
    for line in &page.verdicts.lines {
        out.push_str(&format!("  {}\n", line.replace("**", "")));
    }
    for note in page.notes {
        out.push_str(&format!("  note: {note}\n"));
    }
    for diff in page.diffs {
        out.push_str(&format!("\n{}\n", scenario_heading(diff, page.branch)));
        if let Status::NoBase(why) | Status::NotComparable(why) = &diff.status {
            out.push_str(&format!("  not compared ({why}); this branch alone:\n"));
        }
        let changed = changed_table(diff, None);
        if !changed.rows.is_empty() {
            out.push_str(&changed.text());
        }
        let unchanged = unchanged_line(diff);
        if !unchanged.is_empty() {
            out.push_str(&format!("  {unchanged}\n"));
        }
        let noisy = noisy_table(diff);
        if !noisy.rows.is_empty() {
            out.push_str("  wall time and memory (noisy, never judged):\n");
            out.push_str(&noisy.text());
        }
    }
    out.push('\n');
    for line in timing_lines(page) {
        out.push_str(&format!("{line}\n"));
    }
    out
}

fn side(report: &Value, id: &str) -> Value {
    let Some(scenario) = report["scenarios"]
        .as_array()
        .and_then(|all| all.iter().find(|scenario| scenario["id"] == id))
    else {
        return Value::Null;
    };
    let windows: Vec<Value> = scenario["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            json!({
                "id": w["id"], "from_tick": w["from_tick"], "to_tick": w["to_tick"],
                "service": w["service"], "cost": w["cost"], "efficiency": w["efficiency"],
            })
        })
        .collect();
    let delivered: Vec<&Value> = scenario["series"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|minute| &minute["service"]["economy"]["delivered_total"])
        .collect();
    json!({
        "seats": scenario["seats"], "seed": scenario["seed"], "ticks": scenario["ticks"],
        "start": scenario["start"], "sample_every": scenario["sample_every"],
        "measured_at_workers": scenario["measured_at_workers"],
        "determinism": scenario["determinism"],
        "windows": windows,
        "resources_delivered_per_game_minute": delivered,
    })
}

/// What `make ci-report` reads later: both sides' windows, compactly. `with_base` false drops the
/// merge base's side when the comment would be too long.
fn report_json(page: &Page<'_>, with_base: bool) -> String {
    let scenarios: Vec<Value> = page
        .diffs
        .iter()
        .map(|diff| {
            json!({
                "id": diff.id,
                "branch": page.branch.map_or(Value::Null, |report| side(report, &diff.id)),
                "base": page.base_report.filter(|_| with_base).map_or(Value::Null, |report| side(report, &diff.id)),
            })
        })
        .collect();
    json!({
        "schema": super::super::report::SCHEMA,
        "head": page.head,
        "base": page.base,
        "map_hash": page.map_hash,
        "verdicts": page.verdicts.lines,
        "scenarios": scenarios,
    })
    .to_string()
}

fn body(page: &Page<'_>, json: Option<&str>) -> String {
    let marker = json!({
        "schema": super::super::report::SCHEMA,
        "head": page.head,
        "base": page.base,
        "map_hash": page.map_hash,
    });
    let mut out = format!(
        "{MARKER}\n<!-- {marker} -->\n## Game score (informational)\n\n{}. Nothing here gates the pull request.\n\n**Verdict**\n\n",
        title(page)
    );
    for line in &page.verdicts.lines {
        out.push_str(&format!("- {line}\n"));
    }
    for note in page.notes {
        out.push_str(&format!("- note: {note}\n"));
    }
    for diff in page.diffs {
        out.push_str(&format!(
            "\n### {}\n\n",
            scenario_heading(diff, page.branch)
        ));
        if let Status::NoBase(why) | Status::NotComparable(why) = &diff.status {
            out.push_str(&format!(
                "Not compared ({why}); the numbers of this branch alone:\n\n"
            ));
        }
        let named = changed_table(diff, Some(true));
        if !named.rows.is_empty() {
            out.push_str(&named.markdown());
            out.push('\n');
        }
        // The long tail of a game that went another way stays one click away.
        let rest = changed_table(diff, Some(false));
        if !rest.rows.is_empty() {
            out.push_str(&format!(
                "<details><summary>{} more number{} changed (work per issuer, deliveries by resource, samples)</summary>\n\n{}\n</details>\n\n",
                rest.rows.len(),
                if rest.rows.len() == 1 { "" } else { "s" },
                rest.markdown()
            ));
        }
        let unchanged = unchanged_line(diff);
        if !unchanged.is_empty() {
            out.push_str(&format!("{unchanged}\n\n"));
        }
        let noisy = noisy_table(diff);
        if !noisy.rows.is_empty() {
            out.push_str(
                "<details><summary>Wall time and memory (noisy, never judged)</summary>\n\n",
            );
            out.push_str(&noisy.markdown());
            out.push_str("\n</details>\n");
        }
    }
    out.push_str("\n**How long it took**\n\n");
    for line in timing_lines(page) {
        out.push_str(&format!("- {line}\n"));
    }
    out.push_str(ADDED);
    out.push('\n');
    match json {
        Some(json) => out.push_str(&format!(
            "\n<details><summary>Full report (JSON)</summary>\n\n```json\n{json}\n```\n\n</details>\n"
        )),
        None => out.push_str(
            "\nThe full report is too long for a comment; it is in the results cache of the machine that ran it.\n",
        ),
    }
    out
}

/// The pull request comment: the marker, the verdict, a table per scenario, what it cost, and the
/// report in a collapsed block (the base's side dropped, then the whole block, when too long).
pub(super) fn section(page: &Page<'_>) -> String {
    for json in [
        Some(report_json(page, true)),
        Some(report_json(page, false)),
        None,
    ] {
        let text = body(page, json.as_deref());
        if text.chars().count() <= COMMENT_LIMIT {
            return text;
        }
    }
    body(page, None)
}

#[cfg(test)]
#[path = "tests/render.rs"]
mod tests;
