//! Reviewer sessions: the real runner or scripted answers in tests, and the
//! one retry an answered-but-unreadable session gets.
use super::{
    Result, add_findings, config, increment, prompt,
    protocol::Finding,
    protocol::{Answer, Grade},
    runner,
};
use crate::agents::Runtime;
use std::path::Path;

/// Asks reviewer sessions: the real runner, or scripted answers in tests.
pub(crate) trait Ask: Sync {
    fn all(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>>;

    /// The same, on the runtime's strong model (the arbiter's). Scripted
    /// answers in tests need not tell the two apart.
    fn strong(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>> {
        self.all(prompts)
    }
}

/// An `Ask` whose sessions all run on the strong model, so the arbiter gets
/// the same retry of an unreadable answer as every other session.
pub(super) struct Strong<'a>(pub(super) &'a dyn Ask);

impl Ask for Strong<'_> {
    fn all(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>> {
        self.0.strong(prompts)
    }
}

/// Ask every prompt; a session that answered but whose answer cannot be read
/// is asked once more, told why (a crashed or timed-out session is not).
/// Answers come back in prompt order.
pub(super) fn ask_checked(
    ask: &dyn Ask,
    prompts: Vec<String>,
    readable: fn(&str) -> std::result::Result<(), String>,
) -> Vec<std::result::Result<String, String>> {
    let check = |answer: &std::result::Result<String, String>| match answer {
        Ok(text) => readable(text),
        Err(error) => Err(error.clone()),
    };
    let mut answers = ask.all(prompts.clone());
    let retry: Vec<(usize, String)> = answers
        .iter()
        .enumerate()
        .filter(|(_, answer)| match answer {
            Ok(_) => true,
            Err(error) => error.starts_with(runner::NO_ANSWER),
        })
        .filter_map(|(i, answer)| check(answer).err().map(|e| (i, e)))
        .collect();
    if retry.is_empty() {
        return answers;
    }
    let again = ask.all(
        retry
            .iter()
            .map(|(i, error)| {
                let error: String = error.chars().take(300).collect();
                format!(
                    "{}\n\n## Your previous answer could not be read\n\n{error}\n\nAnswer again: end with exactly one valid JSON object between the markers.\n",
                    prompts[*i]
                )
            })
            .collect(),
    );
    for ((i, first), answer) in retry.into_iter().zip(again) {
        answers[i] = match check(&answer) {
            Ok(()) => answer,
            Err(second) => Err(format!("{first}; asked again: {second}")),
        };
    }
    answers
}

pub(super) fn readable_answer(text: &str) -> std::result::Result<(), String> {
    serde_json::from_str::<Answer>(text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(super) fn readable_grade(text: &str) -> std::result::Result<(), String> {
    serde_json::from_str::<Grade>(text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(super) struct Live<'a> {
    pub(super) runtime: Runtime,
    pub(super) root: &'a Path,
    pub(super) model: config::Model,
    pub(super) strong: config::Model,
}

impl Ask for Live<'_> {
    fn all(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>> {
        runner::ask_all(self.runtime, self.root, &self.model, prompts)
    }

    fn strong(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>> {
        runner::ask_all(self.runtime, self.root, &self.strong, prompts)
    }
}

/// Round 1: every persona reviews the change blind.
pub(super) fn blind_round(
    root: &Path,
    personas: &[String],
    subject: &prompt::Subject,
    ask: &dyn Ask,
    findings: &mut Vec<Finding>,
    failures: &mut Vec<String>,
    filter: &mut increment::Filter,
) -> Result<()> {
    let prompts = personas
        .iter()
        .map(|p| prompt::first_round(root, p, subject))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (reviewer, answer) in ask_checked(ask, prompts, readable_answer)
        .into_iter()
        .enumerate()
    {
        match answer
            .and_then(|text| serde_json::from_str::<Answer>(&text).map_err(|e| e.to_string()))
        {
            Ok(answer) => {
                let inside = filter.admit(answer.findings);
                add_findings(findings, reviewer, 1, inside)
            }
            Err(error) => failures.push(format!("round 1, {}: {error}", personas[reviewer])),
        }
    }
    Ok(())
}
