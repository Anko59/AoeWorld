//! Bounded failure excerpt: the lines of a failed command's output that say why it failed.
use super::{CaptureExit, Captured};

const MAX_LINES: usize = 40;
const MAX_BYTES: usize = 4096;
const MAX_LINE_CHARS: usize = 200;
const FALLBACK_LINES: usize = 15;
/// Lines kept after a `panicked at` line (the assertion message and operands).
const PANIC_CONTEXT: usize = 3;

/// The excerpt to print before the error line, or `None` when the command succeeded or never ran.
pub(super) fn excerpt_for(captured: &Captured, log: &str) -> Option<String> {
    match captured.exit {
        CaptureExit::Failed(_) | CaptureExit::Deadline | CaptureExit::Cancelled => {}
        _ => return None,
    }
    let mut text = String::from_utf8_lossy(&captured.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&captured.stderr));
    let lines = select(&text);
    if lines.is_empty() {
        return None;
    }
    Some(render(lines, log))
}

fn select(text: &str) -> Vec<String> {
    let matched = failure_blocks(text);
    if !matched.is_empty() {
        return matched;
    }
    let tail: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    tail[tail.len().saturating_sub(FALLBACK_LINES)..].to_vec()
}

/// The lines of `text` that say why a command failed, or none when nothing is recognisable.
pub(crate) fn failure_blocks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        let take = if line.contains("panicked at ") {
            let following = lines[index + 1..]
                .iter()
                .take(PANIC_CONTEXT)
                .take_while(|next| !next.trim().is_empty())
                .count();
            1 + following
        } else if line.starts_with("error")
            && (line.starts_with("error[") || line.starts_with("error:"))
        {
            1 + usize::from(
                lines
                    .get(index + 1)
                    .is_some_and(|n| n.trim().starts_with("-->")),
            )
        } else if is_failure_line(line) {
            1
        } else {
            0
        };
        if take == 0 {
            index += 1;
            continue;
        }
        let block: Vec<&str> = lines[index..index + take].to_vec();
        if !blocks.iter().any(|seen| trimmed(seen) == trimmed(&block)) {
            blocks.push(block);
        }
        index += take;
    }
    blocks.into_iter().flatten().map(str::to_owned).collect()
}

/// Names of the failing tests a runner printed (nextest `FAIL [..] name`, libtest `test name ... FAILED`).
pub(crate) fn failed_test_names(text: &str) -> Vec<String> {
    status_names(text, &["FAIL ", "TIMEOUT "], " ... FAILED")
}

/// Names of the passing tests a runner printed (nextest `PASS [..] name`, libtest `test name ... ok`).
pub(crate) fn passed_test_names(text: &str) -> Vec<String> {
    status_names(text, &["PASS "], " ... ok")
}

fn status_names(text: &str, statuses: &[&str], libtest_suffix: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines().map(str::trim) {
        let name = if let Some(status) = statuses.iter().find_map(|s| line.strip_prefix(s)) {
            let name = status
                .split_once(']')
                .map_or(status, |(_, name)| name)
                .trim();
            name.strip_prefix('(')
                .and_then(|s| s.split_once(')'))
                .map_or(name, |(_, name)| name)
                .trim()
        } else if let Some(test) = line
            .strip_prefix("test ")
            .and_then(|s| s.strip_suffix(libtest_suffix))
        {
            test.trim()
        } else {
            continue;
        };
        if !name.is_empty() {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// A line that only reports one passing test; a summary never embeds these.
pub(crate) fn is_passing_line(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("PASS [")
        || line.starts_with("Focused PASS:")
        || (line.starts_with("test ") && line.ends_with(" ... ok"))
        || (line.starts_with('{') && line.contains("\"event\":\"ok\""))
}

/// Blocks repeat with different indentation (runner output vs. its failures summary).
fn trimmed<'a>(block: &[&'a str]) -> Vec<&'a str> {
    block.iter().map(|line| line.trim()).collect()
}

fn is_failure_line(line: &str) -> bool {
    line.starts_with("test result: FAILED")
        || line.starts_with("FAIL [")
        || (line.starts_with("test ") && (line.ends_with(" FAIL") || line.ends_with(" FAILED")))
}

fn render(lines: Vec<String>, log: &str) -> String {
    let total = lines.len();
    let mut out = String::from("--- failure excerpt ---\n");
    let budget = MAX_BYTES;
    let mut shown = 0;
    for line in lines.iter().take(MAX_LINES) {
        let cut: String = if line.chars().count() > MAX_LINE_CHARS {
            let head: String = line.chars().take(MAX_LINE_CHARS).collect();
            format!("{head}…")
        } else {
            line.clone()
        };
        if out.len() + cut.len() + 1 > budget {
            break;
        }
        out.push_str(&cut);
        out.push('\n');
        shown += 1;
    }
    if shown < total {
        out.push_str(&format!("({} more in {log})\n", total - shown));
    }
    out
}
