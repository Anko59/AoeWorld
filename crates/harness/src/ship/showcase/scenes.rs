//! The rendered scene kinds beyond cards, plain terminals and browsers:
//! `timing`, `api`, `agent-session` and the side-by-side terminal. Like cards
//! they are drawn by `scenes.mjs` from storyboard data alone, run nothing and
//! need no network. Their sizes are bounded here, and their natural lengths
//! mirror the recorder's pacing so narration stays aligned with the screen.
use super::{Line, Style, browser::TYPE_MS};
use serde::{Deserialize, Serialize};

/// Recorder pacing, mirrored in `scenes.mjs`.
pub(crate) const HOLD_MS: u64 = 3000;
/// A transcript is typed faster than a command: it is prose.
pub(crate) const SESSION_TYPE_MS: u64 = 10;
const SESSION_PAUSE_MS: u64 = 400;
const RACE_MS: u64 = 2500;
const TIMING_INTRO_MS: u64 = 1000;
const TIMING_HOLD_MS: u64 = 2500;
const EXTRA_MS: u64 = 400;
const REPLY_LINE_MS: u64 = 300;
const REQUEST_PAUSE_MS: u64 = 500;

const MAX_TITLE: usize = 120;
const MAX_COMMAND: usize = 200;
const MAX_MS: u64 = 3_600_000;
const MAX_EXTRA: usize = 4;
const MAX_FIGURE: usize = 40;
const MAX_PATH: usize = 200;
const MAX_REQUEST_LINES: usize = 20;
const MAX_REPLY_LINES: usize = 22;
const MAX_BODY_LINE: usize = 80;
const MAX_BODY_CHARS: usize = 2000;
pub(crate) const MAX_STEPS: usize = 40;
pub(crate) const MAX_STEP_CHARS: usize = 600;
pub(crate) const MAX_SESSION_CHARS: usize = 6000;
const MAX_PANE_LINES: usize = 30;
const MAX_PANE_LINE: usize = 100;
const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// How a `timing` scene prints its two durations (the inputs are in ms).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Unit {
    #[default]
    Ms,
    S,
}

/// One more before/after pair shown under the race (memory, requests, …).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Figure {
    pub(crate) label: String,
    pub(crate) before: String,
    pub(crate) after: String,
}

/// One response of an `api` scene.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) body: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    User,
    Agent,
    Tool,
    Hook,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Verdict {
    Allowed,
    Denied,
}

/// One entry of an `agent-session` transcript.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionStep {
    pub(crate) role: Role,
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) verdict: Option<Verdict>,
}

/// The two panes of a side-by-side terminal.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Compare {
    pub(crate) before: Vec<Line>,
    pub(crate) after: Vec<Line>,
}

fn chars(text: &str) -> u64 {
    text.chars().count() as u64
}

/// Lines of a body as the recorder splits it.
fn body_lines(text: &str) -> usize {
    text.split('\n').count()
}

/// What a terminal pane needs on screen, before the final hold.
pub(crate) fn lines_ms(lines: &[Line]) -> u64 {
    lines
        .iter()
        .map(|line| match line.style {
            Style::Cmd => TYPE_MS * chars(&line.text) + 500,
            _ if line.text.is_empty() => 200,
            _ => 650,
        })
        .sum()
}

/// Both panes of a comparison play at once; the longer one sets the length.
pub(crate) fn compare_ms(compare: &Compare) -> u64 {
    lines_ms(&compare.before).max(lines_ms(&compare.after)) + HOLD_MS
}

pub(crate) fn timing_ms(extras: usize) -> u64 {
    TIMING_INTRO_MS + RACE_MS + EXTRA_MS * extras as u64 + TIMING_HOLD_MS
}

pub(crate) fn api_ms(method: &str, path: &str, before: &Reply, after: &Reply) -> u64 {
    let rows = body_lines(&before.body).max(body_lines(&after.body)) as u64;
    TYPE_MS * (chars(method) + 1 + chars(path)) + REQUEST_PAUSE_MS + REPLY_LINE_MS * rows + HOLD_MS
}

pub(crate) fn session_ms(steps: &[SessionStep]) -> u64 {
    let shown: u64 = steps
        .iter()
        .map(|step| SESSION_TYPE_MS * chars(&step.text) + SESSION_PAUSE_MS)
        .sum();
    shown + HOLD_MS
}

fn text_within(what: &str, text: &str, limit: usize) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("{what} is empty"));
    }
    if text.chars().count() > limit {
        return Err(format!("{what} is over {limit} characters"));
    }
    if text.chars().any(|c| c.is_control()) {
        return Err(format!("{what} has a control character"));
    }
    Ok(())
}

pub(crate) fn validate_timing(
    label: &str,
    command: &str,
    before_ms: u64,
    after_ms: u64,
    extra: &[Figure],
) -> Result<(), String> {
    text_within("timing label", label, MAX_TITLE)?;
    text_within("timing command", command, MAX_COMMAND)?;
    for (name, ms) in [("before_ms", before_ms), ("after_ms", after_ms)] {
        if !(1..=MAX_MS).contains(&ms) {
            return Err(format!("timing {name} must be 1 to {MAX_MS} ms"));
        }
    }
    if extra.len() > MAX_EXTRA {
        return Err(format!("timing has more than {MAX_EXTRA} extra figures"));
    }
    for figure in extra {
        for (what, text) in [
            ("label", &figure.label),
            ("before", &figure.before),
            ("after", &figure.after),
        ] {
            text_within(&format!("timing extra {what}"), text, MAX_FIGURE)?;
        }
    }
    Ok(())
}

fn validate_body(
    what: &str,
    body: &str,
    max_lines: usize,
    allow_empty: bool,
) -> Result<(), String> {
    if body.is_empty() && allow_empty {
        return Ok(());
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(format!("{what} is over {MAX_BODY_CHARS} characters"));
    }
    if body.chars().any(|c| c.is_control() && c != '\n') {
        return Err(format!("{what} has a control character"));
    }
    if body_lines(body) > max_lines {
        return Err(format!("{what} is over {max_lines} lines"));
    }
    if body
        .split('\n')
        .any(|line| line.chars().count() > MAX_BODY_LINE)
    {
        return Err(format!("{what} has a line over {MAX_BODY_LINE} characters"));
    }
    Ok(())
}

pub(crate) fn validate_api(
    caption: &str,
    method: &str,
    path: &str,
    request: Option<&str>,
    before: &Reply,
    after: &Reply,
) -> Result<(), String> {
    text_within("api caption", caption, MAX_TITLE)?;
    if !METHODS.contains(&method) {
        return Err(format!("api method `{method}` is not one of {METHODS:?}"));
    }
    text_within("api path", path, MAX_PATH)?;
    if !path.starts_with('/') || path.contains(char::is_whitespace) {
        return Err("api path must start with `/` and have no spaces".into());
    }
    if let Some(request) = request {
        validate_body("api request", request, MAX_REQUEST_LINES, false)?;
    }
    for (name, reply) in [("before", before), ("after", after)] {
        if !(100..=599).contains(&reply.status) {
            return Err(format!("api {name} status must be 100 to 599"));
        }
        validate_body(
            &format!("api {name} body"),
            &reply.body,
            MAX_REPLY_LINES,
            true,
        )?;
    }
    Ok(())
}

pub(crate) fn validate_session(
    tag: &str,
    caption: &str,
    steps: &[SessionStep],
) -> Result<(), String> {
    text_within("session tag", tag, MAX_TITLE)?;
    text_within("session caption", caption, MAX_TITLE)?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(format!("a session has 1 to {MAX_STEPS} steps"));
    }
    let mut total = 0;
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        if step.text.trim().is_empty() || chars(&step.text) > MAX_STEP_CHARS as u64 {
            return Err(format!(
                "session step {number}'s text must be 1 to {MAX_STEP_CHARS} characters"
            ));
        }
        if step.text.chars().any(|c| c.is_control() && c != '\n') {
            return Err(format!("session step {number} has a control character"));
        }
        if step.verdict.is_some() && !matches!(step.role, Role::Tool | Role::Hook) {
            return Err(format!(
                "session step {number}: only tool and hook steps carry a verdict"
            ));
        }
        total += step.text.chars().count();
    }
    if total > MAX_SESSION_CHARS {
        return Err(format!(
            "a session has at most {MAX_SESSION_CHARS} characters"
        ));
    }
    Ok(())
}

/// A terminal scene shows its own `lines` or a `compare`, never both.
pub(crate) fn validate_terminal(lines: &[Line], compare: Option<&Compare>) -> Result<(), String> {
    match (lines.is_empty(), compare) {
        (false, Some(_)) => Err("terminal takes `lines` or `compare`, not both".into()),
        (_, Some(compare)) => {
            for (side, pane) in [("before", &compare.before), ("after", &compare.after)] {
                if pane.is_empty() || pane.len() > MAX_PANE_LINES {
                    return Err(format!(
                        "compare {side} pane has 1 to {MAX_PANE_LINES} lines"
                    ));
                }
                if pane.iter().any(|l| l.text.chars().count() > MAX_PANE_LINE) {
                    return Err(format!(
                        "compare {side} pane has a line over {MAX_PANE_LINE} characters"
                    ));
                }
            }
            Ok(())
        }
        (_, None) => Ok(()),
    }
}
