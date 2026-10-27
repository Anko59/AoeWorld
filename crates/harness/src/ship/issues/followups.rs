//! File passing-review findings in the configured workstream issues.
use super::{
    Result, fingerprint,
    routing::{self, Config},
};
use crate::review::{
    Report,
    protocol::{Finding, Severity, Status},
};
use std::{collections::HashSet, path::Path};

pub(in crate::ship) fn file(root: &Path, url: &str, report: &Report) -> Result<()> {
    if url.chars().count() > 1_000 {
        return Err("pull request URL exceeds 1000 characters".into());
    }
    let pr = pr_number(url).ok_or_else(|| format!("no pull request number in {url}"))?;
    let config = Config::load(root)?;
    for (number, game) in [(config.game_review, true), (config.other_review, false)] {
        let mut findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| matches!(finding.status, Status::Confirmed | Status::Disputed))
            .filter(|finding| (config.review(&finding.reported.file) == config.game_review) == game)
            .collect();
        findings.sort_by_key(|finding| match finding.reported.severity {
            Severity::Critical => 0,
            Severity::Major => 1,
            Severity::Minor => 2,
            Severity::Nit => 3,
        });
        let mut seen = HashSet::new();
        let lines: Vec<_> = findings
            .into_iter()
            .filter(|finding| {
                let title = clip(
                    &format!(
                        "Review finding at {}: {}",
                        location(finding),
                        inline(&finding.reported.claim)
                    ),
                    super::TITLE_LIMIT,
                );
                seen.insert(fingerprint(&title))
            })
            .map(item)
            .collect();
        if !lines.is_empty() {
            let heading = format!(
                "**PR #{pr} — {}**\n\nPassing review at {}:\n",
                inline(url),
                clip(&inline(&report.head), 600)
            );
            let mut body = heading;
            let total = lines.len();
            for (index, line) in lines.iter().enumerate() {
                let remaining = total - index - 1;
                let overflow = format!("\n… {remaining} more findings, see {}", inline(url));
                if body.chars().count() + line.chars().count() + overflow.chars().count() + 1
                    > 50_000
                {
                    body.push_str(&format!(
                        "\n… {} more findings, see {}",
                        total - index,
                        inline(url)
                    ));
                    break;
                }
                body.push_str(line);
                body.push('\n');
            }
            routing::comment(root, number, &body);
        }
    }
    Ok(())
}

fn pr_number(url: &str) -> Option<u64> {
    let (rest, number) = url.trim_end_matches('/').rsplit_once('/')?;
    let number = number.parse().ok().filter(|n| *n > 0)?;
    rest.ends_with("/pull").then_some(number)
}

fn item(finding: &Finding) -> String {
    let location = location(finding);
    let severity = match finding.reported.severity {
        Severity::Critical => "critical",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Nit => "nit",
    };
    let mut text = format!(
        "- [ ] {location} — {} ({severity}, {})",
        clip(&inline(&finding.reported.claim), 300),
        if finding.status == Status::Confirmed {
            "confirmed"
        } else {
            "disputed"
        }
    );
    for (label, value) in [
        ("Category", &finding.reported.category),
        ("Trigger", &finding.reported.trigger),
        ("Expected vs actual", &finding.reported.expected_vs_actual),
        ("Evidence", &finding.reported.evidence),
    ] {
        if !value.trim().is_empty() {
            text.push_str(&format!("\n  - {label}: {}", clip(&inline(value), 600)));
        }
    }
    text
}

fn location(finding: &Finding) -> String {
    let file = clip(&inline(&finding.reported.file), 160);
    match finding.reported.line {
        Some(line) => format!("{file}:{line}"),
        None => file,
    }
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    format!("{}…", text.chars().take(limit - 1).collect::<String>())
}

fn inline(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '<' => "&lt;".to_owned(),
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}' => " ".to_owned(),
            c if c.is_control() => " ".to_owned(),
            c => c.to_string(),
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "tests/followups.rs"]
mod tests;
