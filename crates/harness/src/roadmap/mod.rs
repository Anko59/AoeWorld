//! `make roadmap` and `make roadmap-check` (docs/roadmap.md): the status-tracked roadmap in
//! `docs/roadmap/*.md`. The first prints the open items in file order (the owner's priority order)
//! with counts per status; the second fails when a DONE row names no pull request or names one
//! that is not merged.
mod table;

#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, fs, path::Path};
use table::{Row, Status};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Area files in priority order; anything else in the directory follows alphabetically.
// Owner, 2026-11-16: "the real issue is the tick rate" — scale comes first.
const AREAS: [&str; 8] = [
    "scale",
    "movement",
    "gameplay",
    "ai",
    "hierarchy",
    "ui",
    "map",
    "harness",
];

pub(crate) fn print() -> Result<()> {
    let rows = load(Path::new("docs/roadmap"))?;
    print!("{}", report(&rows));
    Ok(())
}

pub(crate) fn check() -> Result<()> {
    let rows = load(Path::new("docs/roadmap"))?;
    let merged = if rows.iter().any(|row| row.status == Status::Done) {
        merged_pull_requests()?
    } else {
        BTreeSet::new()
    };
    let problems = problems(&rows, &merged);
    if problems.is_empty() {
        println!(
            "roadmap: {} rows, every DONE row names merged pull requests",
            rows.len()
        );
        return Ok(());
    }
    for problem in &problems {
        eprintln!("roadmap: {problem}");
    }
    Err(format!("{} roadmap rows are not honest", problems.len()).into())
}

fn load(directory: &Path) -> Result<Vec<Row>> {
    let mut names: Vec<String> = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".md").map(str::to_owned))
        .collect();
    names.sort_by_key(|name| {
        (
            AREAS
                .iter()
                .position(|area| area == name)
                .unwrap_or(AREAS.len()),
            name.clone(),
        )
    });
    let mut rows = Vec::new();
    for name in names {
        let text = fs::read_to_string(directory.join(format!("{name}.md")))?;
        rows.extend(table::parse(&name, &text));
    }
    Ok(rows)
}

/// Pull request numbers GitHub reports as merged, read through the shared `gh` caller.
fn merged_pull_requests() -> Result<BTreeSet<u64>> {
    let reply = crate::ship::gh(
        Path::new("."),
        &[
            "pr", "list", "--state", "merged", "--limit", "5000", "--json", "number",
        ],
    )?;
    parse_merged(&reply)
}

fn parse_merged(reply: &str) -> Result<BTreeSet<u64>> {
    #[derive(serde::Deserialize)]
    struct Number {
        number: u64,
    }
    let numbers: Vec<Number> = serde_json::from_str(reply)?;
    Ok(numbers.into_iter().map(|entry| entry.number).collect())
}

fn problems(rows: &[Row], merged: &BTreeSet<u64>) -> Vec<String> {
    let mut found = Vec::new();
    for row in rows {
        let place = format!("{}:{} {}", row.area, row.line, row.item);
        if row.status == Status::Unknown {
            found.push(format!("{place}: status must start with DONE, PARTIAL, IN FLIGHT, PLANNED, DROPPED, SUPERSEDED or UNVERIFIED"));
        }
        if row.status != Status::Done {
            continue;
        }
        let numbers = table::pull_requests(&row.evidence);
        if numbers.is_empty() {
            found.push(format!("{place}: DONE names no pull request number"));
        }
        for number in numbers.iter().filter(|number| !merged.contains(number)) {
            found.push(format!("{place}: #{number} is not a merged pull request"));
        }
    }
    found
}

fn report(rows: &[Row]) -> String {
    let mut out = String::from("Roadmap status counts\n");
    for status in Status::ALL {
        let count = rows.iter().filter(|row| row.status == status).count();
        out.push_str(&format!("  {:<11} {count}\n", status.label()));
    }
    out.push_str("\nOpen items by priority (area file order, then row order)\n");
    let mut number = 0;
    for row in rows.iter().filter(|row| row.status.is_open()) {
        number += 1;
        out.push_str(&format!(
            "{number:>3}. [{}] {} / {}\n       next: {}\n",
            row.status.label(),
            row.area,
            row.item,
            if row.next.is_empty() { "-" } else { &row.next }
        ));
    }
    out
}
