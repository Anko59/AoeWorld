//! Panics in a wasm run's output: where they were raised and what they asserted.
use super::super::focus::Panic;

/// Most panics and assertion lines a report keeps.
const PANICS: usize = 5;
const MESSAGE_LINES: usize = 3;

/// Every `panicked at <path>:<line>:<col>` with the lines after it, bounded and without duplicates.
pub(super) fn parse(output: &str) -> Vec<Panic> {
    let lines: Vec<_> = output.lines().collect();
    let mut found: Vec<Panic> = Vec::new();
    let mut test: Option<String> = None;
    for (at, line) in lines.iter().enumerate() {
        match owner(line) {
            Owner::Failing(name) => test = Some(name),
            Owner::Passing => test = None,
            Owner::None => {}
        }
        let Some((_, rest)) = line.split_once("panicked at ") else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(':');
        let mut parts = rest.rsplitn(3, ':');
        let (Some(column), Some(row), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if column.parse::<u32>().is_err() || row.parse::<u32>().is_err() || path.is_empty() {
            continue;
        }
        let message = lines[at + 1..]
            .iter()
            .map(|l| l.trim())
            .take_while(|l| !l.is_empty() && !l.starts_with("note:") && !l.contains("panicked at "))
            .take(MESSAGE_LINES)
            .map(|l| match l.char_indices().nth(240) {
                Some((end, _)) => format!("{}…", &l[..end]),
                None => l.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let path = normalize(path);
        let panic = Panic {
            location: format!("{path}:{row}:{column}"),
            path,
            message,
            test: test.clone(),
        };
        if !found.contains(&panic) {
            found.push(panic);
        }
        if found.len() == PANICS {
            break;
        }
    }
    found
}

/// Resolve `.` and `..` components lexically: `crates/a/tests/../../tests/x.rs` is `crates/tests/x.rs`.
pub(crate) fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

enum Owner {
    Failing(String),
    Passing,
    None,
}

/// Which test a line hands the following panics to: a failing test's `FAIL` line or `---- name output ----`
/// header, or a passing test's line (after which a panic belongs to no named test).
fn owner(line: &str) -> Owner {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("test ")
        && let Some((name, status)) = rest.rsplit_once(" ... ")
    {
        return match status.trim() {
            "FAIL" | "FAILED" => Owner::Failing(name.to_owned()),
            "ok" => Owner::Passing,
            _ => Owner::None,
        };
    }
    if let Some(rest) = line.strip_prefix("---- ")
        && let Some(name) = rest
            .strip_suffix(" output ----")
            .or_else(|| rest.strip_suffix(" stdout ----"))
    {
        return Owner::Failing(name.to_owned());
    }
    Owner::None
}
