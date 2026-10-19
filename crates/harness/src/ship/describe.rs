//! The pull request description (docs/shipping.md). The agent writes
//! `SHIP_BODY` with a level and three sections; the harness checks the limits
//! a human reader relies on and renders two parts: "For humans" (Why, What with
//! the video, How with the review grade and summary) and "For AI"
//! (the agent's notes, the test-first report, gate evidence and the full
//! review).
//!
//! ```markdown
//! <!-- level: medium -->
//! ## Why
//! > "quote from the request", an issue/reference link, a screenshot or a sampled metric
//! At most two lines saying why the PR is needed.
//! ## What
//! At most two lines saying what was implemented.
//! ## For AI
//! Anything else: design, alternatives, risks, follow-ups.
//! ```
use super::{
    evidence::Evidence,
    levels::{Change, Floor, Levels},
    run::GateVerdict,
};
use crate::review::Report;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Level {
    Low,
    Medium,
    High,
    Max,
}

impl Level {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
    /// Longest video in seconds; `None`: no video at this level.
    pub(crate) fn video_limit(self) -> Option<u32> {
        Levels::embedded()?.spec(self).video_max_seconds
    }
    /// Whether the video needs a voice-over.
    pub(crate) fn voiced(self) -> bool {
        Levels::embedded().is_some_and(|levels| levels.spec(self).narration_required)
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Template {
    pub(crate) level: Level,
    pub(crate) why: String,
    pub(crate) what: String,
    pub(crate) for_ai: String,
}

fn section(text: &str, name: &str) -> Option<String> {
    let normalized = text.replace("\r\n", "\n");
    let start = normalized.find(&format!("## {name}\n"))? + name.len() + 4;
    // The section ends at the next `## ` heading, even one right after it.
    let rest = &normalized[start..];
    let end = if rest.starts_with("## ") {
        start
    } else {
        rest.find("\n## ")
            .map_or(normalized.len(), |i| start + i + 1)
    };
    Some(normalized[start..end].trim().to_owned())
}

/// A quote, issue/reference link, image or metric line that grounds the "Why".
fn is_element(line: &str) -> bool {
    let line = line.trim();
    let issue = line
        .split('#')
        .skip(1)
        .any(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()));
    let markdown_link = line
        .find('[')
        .and_then(|open| {
            let text_end = line[open + 1..].find(']')? + open + 1;
            if text_end == open + 1 || line.as_bytes().get(text_end + 1) != Some(&b'(') {
                return None;
            }
            let url_start = text_end + 2;
            let url_end = line[url_start..].find(')')? + url_start;
            (!line[url_start..url_end].trim().is_empty()).then_some(())
        })
        .is_some();
    let url = line
        .split_whitespace()
        .any(|token| token.starts_with("https://"));
    let sampled_metric = line.split_whitespace().any(|token| {
        token
            .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
            .parse::<f64>()
            .is_ok()
    }) && line.split_whitespace().any(|token| {
        let unit = token.trim_matches(|c: char| !c.is_ascii_alphabetic() && c != '%');
        unit == "%"
            || matches!(
                unit.to_ascii_lowercase().as_str(),
                "ms" | "s" | "sec" | "seconds" | "fps" | "mb" | "gb" | "kb" | "bytes" | "entities"
            )
    });
    (line.starts_with('>') && !line.trim_start_matches('>').trim().is_empty())
        || line.starts_with("![")
        || markdown_link
        || issue
        || url
        || sampled_metric
}

/// What reading a description found: every problem, not only the first.
pub(crate) struct Inspected {
    pub(crate) level: Option<Level>,
    pub(crate) problems: Vec<String>,
    pub(crate) template: Option<Template>,
}

impl Template {
    /// The first problem of the text, or the template.
    pub(crate) fn parse(text: &str, levels: &Levels) -> Result<Self, String> {
        let found = Self::inspect(text, levels);
        match (found.problems.into_iter().next(), found.template) {
            (Some(problem), _) => Err(problem),
            (None, Some(template)) => Ok(template),
            (None, None) => Err("the description could not be read".into()),
        }
    }

    /// Every problem of the text; the template only when there is none.
    pub(crate) fn inspect(text: &str, levels: &Levels) -> Inspected {
        let mut problems = Vec::new();
        let level = text.lines().find_map(|l| {
            l.trim()
                .strip_prefix("<!-- level:")?
                .strip_suffix("-->")
                .and_then(Level::parse)
        });
        if level.is_none() {
            problems.push(
                "the description needs a first line `<!-- level: low|medium|high|max -->`"
                    .to_owned(),
            );
        }
        let why = section(text, "Why");
        let what = section(text, "What");
        if why.is_none() {
            problems.push("the description needs a `## Why` section".to_owned());
        }
        if what.is_none() {
            problems.push("the description needs a `## What` section".to_owned());
        }
        // Without a level the lowest one's limits stand in for the chosen one.
        let spec = levels.spec(level.unwrap_or(Level::Low));
        let lines = |text: &str| text.lines().filter(|l| !l.trim().is_empty()).count();
        if let Some(what) = &what {
            if what.trim().is_empty() {
                problems.push(
                    "`## What` is empty: say in one or two lines what was implemented".into(),
                );
            } else if lines(what) > spec.what_max_lines {
                problems.push(format!(
                    "`## What` has more than {} lines; if it needs more, split the PR",
                    spec.what_max_lines
                ));
            }
        }
        if let Some(why) = &why {
            let grounding_lines = why.lines().filter(|line| is_element(line)).count();
            if grounding_lines == 0 {
                problems.push("`## Why` needs one grounding element: a > quote from a human, an issue/reference link or HTTPS URL, a screenshot or a sampled metric".to_owned());
            }
            if grounding_lines > 1 || lines(why) > spec.why_max_lines {
                problems.push(format!(
                    "`## Why` has more than {} lines of text; if it needs more, the PR is doing too much: split it",
                    spec.why_max_lines - 1
                ));
            }
        }
        let template = match (level, why, what) {
            (Some(level), Some(why), Some(what)) if problems.is_empty() => Some(Self {
                level,
                why,
                what,
                for_ai: section(text, "For AI").unwrap_or_default(),
            }),
            _ => None,
        };
        Inspected {
            level,
            problems,
            template,
        }
    }

    /// The first way the level fails the change and the video, or what to
    /// record; `ship::check` reports all of them.
    #[cfg(test)]
    pub(crate) fn check(
        &self,
        levels: &Levels,
        change: &Change,
        video: Option<(u32, bool)>,
    ) -> Result<super::levels::Decision, String> {
        let (problems, floor) = level_problems(self.level, levels, change, video.map(Some));
        if let Some(problem) = problems.into_iter().next() {
            return Err(problem);
        }
        Ok(super::levels::Decision {
            chosen: self.level,
            floor: floor.level,
            reason: floor.reason,
            metrics: levels.spec(self.level).metrics,
        })
    }
}

/// Every way `level` fails the change and the video (`Some(None)`: a video
/// that could not be read, already reported), and the floor.
pub(crate) fn level_problems(
    level: Level,
    levels: &Levels,
    change: &Change,
    video: Option<Option<(u32, bool)>>,
) -> (Vec<String>, Floor) {
    let floor = levels.floor(change);
    let name = level.name();
    let mut problems = Vec::new();
    if level < floor.level {
        problems.push(format!(
            "`{name}` is below the floor `{}` for this change ({} changed lines): {}; use {} or higher",
            floor.level.name(),
            change.lines,
            floor.reason,
            floor.level.name()
        ));
    }
    let spec = levels.spec(level);
    match (spec.video_max_seconds, video) {
        (None, Some(_)) => problems.push(format!(
            "{name} PRs have no video; use {} or higher",
            levels.lowest_with_video().name()
        )),
        (Some(_), None) if change.visible() => problems.push(format!(
            "a {name} PR needs a showcase video (SHIP_VIDEO); show it working, before/after"
        )),
        (Some(limit), Some(Some((seconds, voiced)))) => {
            if seconds > limit {
                problems.push(format!(
                    "the video is {seconds}s; a {name} PR allows {limit}s: tighten it or split the PR"
                ));
            }
            if !voiced && spec.narration_required {
                problems.push(format!("a {name} PR video needs a voice-over"));
            }
        }
        _ => {}
    }
    (problems, floor)
}

/// The probe of a video: duration in seconds and whether it has an audio stream,
/// from ffmpeg's banner (`make video-probe`).
pub(crate) fn parse_probe(output: &str) -> Option<(u32, bool)> {
    let first_input = output.split("Input #").nth(1)?;
    let first_input = first_input.split("Input #").next().unwrap_or(first_input);
    let duration = first_input
        .lines()
        .find(|line| line.trim_start().starts_with("Duration:"))?
        .split_once("Duration:")?
        .1
        .split(',')
        .next()?;
    let mut parts = duration.trim().split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    let total = (hours * 3600.0 + minutes * 60.0 + seconds).ceil() as u32;
    let streams: Vec<&str> = output
        .lines()
        .filter(|line| line.trim_start().starts_with("Stream #"))
        .collect();
    streams
        .iter()
        .any(|line| line.contains(": Video:"))
        .then(|| (total, streams.iter().any(|line| line.contains(": Audio:"))))
}

fn attachment_urls(body: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for line in body.lines() {
        let mut rest = line;
        while let Some(start) = rest.find("https://github.com/user-attachments/") {
            rest = &rest[start..];
            let end = rest.find([')', '>', ' ', '\"']).unwrap_or(rest.len());
            urls.push(rest[..end].to_owned());
            rest = &rest[end..];
        }
    }
    urls
}

/// Place only an attachment URL newly added by `gh pr edit --attach` into What.
pub(crate) fn place_video_in_what(before: &str, after: &str) -> String {
    let mut existing = attachment_urls(before);
    let added: Vec<String> = attachment_urls(after)
        .into_iter()
        .filter(|url| {
            if let Some(index) = existing.iter().position(|old| old == url) {
                existing.remove(index);
                false
            } else {
                true
            }
        })
        .collect();
    let Some(url) = added.first() else {
        return after.to_owned();
    };
    let mut lines: Vec<String> = after
        .lines()
        .filter_map(|line| {
            if added.iter().any(|url| line.contains(url)) {
                let keeps_existing = attachment_urls(line)
                    .iter()
                    .any(|url| attachment_urls(before).contains(url));
                keeps_existing.then(|| {
                    added
                        .iter()
                        .fold(line.to_owned(), |line, url| line.replace(url, ""))
                })
            } else {
                Some(line.to_owned())
            }
        })
        .collect();
    let what = lines.iter().position(|line| line.trim() == "### 🛠️ What");
    let Some(index) = what else {
        return after.to_owned();
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, line)| line.starts_with("### "))
        .map_or(lines.len(), |(i, _)| i);
    let mut section = lines[index + 1..end].to_vec();
    section.retain(|line| !line.contains("![Showcase]") && !line.contains("🎬 Showcase"));
    while section.first().is_some_and(|line| line.trim().is_empty()) {
        section.remove(0);
    }
    while section.last().is_some_and(|line| line.trim().is_empty()) {
        section.pop();
    }
    let mut replacement = vec![String::new()];
    replacement.extend(section);
    lines.splice(index + 1..end, replacement);
    let insert_at = lines
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, line)| line.starts_with("### "))
        .map_or(lines.len(), |(i, _)| i);
    let mut insert_at = insert_at;
    while insert_at > index + 1 && lines[insert_at - 1].trim().is_empty() {
        lines.remove(insert_at - 1);
        insert_at -= 1;
    }
    let mut showcase = vec![
        String::new(),
        "🎬 Showcase:".to_owned(),
        String::new(),
        url.to_owned(),
        String::new(),
    ];
    if insert_at == lines.len() {
        showcase.push(String::new());
    }
    lines.splice(insert_at..insert_at, showcase);
    lines.join("\n")
}

pub(crate) fn footer(runtime: crate::agents::Runtime) -> String {
    match runtime {
        crate::agents::Runtime::Claude => {
            "🤖 Generated with [Claude Code](https://claude.com/claude-code)".to_owned()
        }
        other => format!("🤖 Generated with {} through `make ship`", other.label()),
    }
}

/// The generated parts of the description besides the review.
pub(crate) struct Extras<'a> {
    pub(crate) footer: &'a str,
    pub(crate) metrics: &'a str,
    pub(crate) test_first: &'a str,
    /// The level distribution line (`make level-stats`).
    pub(crate) distribution: &'a str,
}

/// The "For AI" lines on the chosen level, its floor and the distribution.
pub(crate) fn level_section(
    template: &Template,
    evidence: &Evidence,
    distribution: &str,
) -> String {
    let mut out = format!("## Level\n\nChosen: `{}`.", template.level.name());
    if let Some(decision) = &evidence.level {
        out.push_str(&format!(
            " Floor: `{}` ({}). Metrics rows: `{}`.",
            decision.floor.name(),
            decision.reason,
            decision.metrics.name()
        ));
    }
    out.push_str(&format!("\n\n{distribution}\n\n"));
    out
}

pub(crate) fn render(
    template: &Template,
    video: Option<&str>,
    report: &Report,
    evidence: &Evidence,
    extras: &Extras,
) -> String {
    // The marker lets `make level-stats` read the level of merged pull requests.
    let mut out = format!("<!-- level: {} -->\n# 🧑 For humans", template.level.name());
    out.push_str(&format!(
        "\n\n### 🎯 Why\n\n{}\n\n### 🛠️ What\n\n{}\n",
        template.why, template.what
    ));
    if let Some(name) = video {
        out.push_str(&format!("\n🎬 ![Showcase](./{name})\n"));
    }
    out.push_str(&format!(
        "\n### ✅ How\n\n{}\n\n{}> {}\n\n{}\n\n---\n\n# 🤖 For AI\n\n",
        report.headline(),
        report.provenance(),
        report.summary.trim().replace('\n', "\n> "),
        extras.metrics,
    ));
    if !template.for_ai.is_empty() {
        out.push_str(&format!("{}\n\n", template.for_ai));
    }
    out.push_str(&level_section(template, evidence, extras.distribution));
    out.push_str(&format!(
        "## Test-first report\n\nReported, never enforced: the test-integrity reviewer judges it.\n\n{}\n",
        extras.test_first
    ));
    out.push_str(&format!(
        "## Gate evidence (`{}`, level {})\n\n| Gate | Verdict | Time |\n|---|---|---|\n",
        &evidence.head[..12],
        template.level.name()
    ));
    for gate in &evidence.gates {
        let verdict = match gate.verdict {
            GateVerdict::Pass => "✅ PASS",
            GateVerdict::Fail => "❌ FAIL",
            GateVerdict::Unavailable => "⚠️ UNAVAILABLE",
        };
        out.push_str(&format!(
            "| {} | {verdict} | {:.0}s |\n",
            gate.gate, gate.seconds
        ));
    }
    out.push_str(&super::recheck::section(evidence));
    out.push_str(&format!(
        "\n## Adversarial review\n\n{}\n\n{}\n",
        report.markdown(),
        extras.footer
    ));
    out
}
