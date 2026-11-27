//! Declarations: how a pull request says that a number of the game score got worse on purpose.
//!
//! One line of the pull request description per number:
//!
//! ```text
//! game-score: late arrived -3% because the AI now waits for its escort before it marches
//! game-score: natural/early first-step-p95 +20% because orders go through the new planner queue
//! ```
//!
//! `<scenario>` (or `<scenario>/<window>` for one window; without a window every named window of
//! the scenario), the number's name (`numbers::KNOWN`), the change in percent against the merge
//! base, then `because` and a reason. A bullet, backticks or capitals around the line are fine.

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Declaration {
    pub scenario: String,
    pub window: Option<String>,
    pub number: String,
    /// The change the pull request admits to, in percent (negative: lower).
    pub percent: f64,
    pub reason: String,
}

impl Declaration {
    /// The line that makes this declaration, as the table shows it.
    pub(super) fn line(&self) -> String {
        let place = match &self.window {
            Some(window) => format!("{}/{window}", self.scenario),
            None => self.scenario.clone(),
        };
        format!(
            "game-score: {place} {} {:+.0}% because {}",
            self.number, self.percent, self.reason
        )
    }
}

const PREFIX: &str = "game-score:";

/// The declarations of a description, and the lines that start like one but cannot be read.
pub(super) fn parse(description: &str) -> (Vec<Declaration>, Vec<String>) {
    let (mut declared, mut unreadable) = (Vec::new(), Vec::new());
    for line in description.lines() {
        let text = line
            .trim()
            .trim_start_matches(['-', '*', '>', ' '])
            .trim_matches('`')
            .trim();
        if !text.to_ascii_lowercase().starts_with(PREFIX) {
            continue;
        }
        match one(&text[PREFIX.len()..]) {
            Some(declaration) => declared.push(declaration),
            None => unreadable.push(text.to_owned()),
        }
    }
    (declared, unreadable)
}

fn one(rest: &str) -> Option<Declaration> {
    let (head, reason) = rest.split_once(" because ")?;
    let reason = reason.trim().trim_end_matches('`').trim();
    let mut words = head.split_whitespace();
    let place = words.next()?;
    let number = words.next()?;
    let percent = words.next()?;
    if words.next().is_some() || reason.is_empty() {
        return None;
    }
    let (scenario, window) = match place.split_once('/') {
        Some((scenario, window)) => (scenario, Some(window.to_ascii_lowercase())),
        None => (place, None),
    };
    let percent = percent
        .strip_suffix('%')?
        .replace('\u{2212}', "-")
        .parse::<f64>()
        .ok()?;
    (!scenario.is_empty() && window.as_deref() != Some("")).then(|| Declaration {
        scenario: scenario.to_ascii_lowercase(),
        window,
        number: number.to_ascii_lowercase(),
        percent,
        reason: reason.to_owned(),
    })
}

#[cfg(test)]
#[path = "tests/declare.rs"]
mod tests;
