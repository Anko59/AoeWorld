//! Reviewer sessions: the real runner or scripted answers in tests, and the
//! one retry an answered-but-unreadable session gets.
use super::{
    config,
    protocol::{Answer, Grade},
    runner,
};
use crate::agents::Runtime;
use std::path::Path;

/// Asks reviewer sessions: the real runner, or scripted answers in tests.
pub(crate) trait Ask: Sync {
    fn all(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>>;
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
}

impl Ask for Live<'_> {
    fn all(&self, prompts: Vec<String>) -> Vec<std::result::Result<String, String>> {
        runner::ask_all(self.runtime, self.root, &self.model, prompts)
    }
}
