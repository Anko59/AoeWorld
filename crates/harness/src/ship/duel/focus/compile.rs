//! Compile errors read from cargo's human output: where each is and which items it names.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct CompileError {
    /// The file the error points at, as cargo prints it (workspace-relative); empty when none follows.
    pub(crate) path: String,
    /// `E0425`; none for an error without a code.
    pub(crate) code: Option<String>,
    /// Every backticked name in the message: the items the code asked for.
    pub(crate) items: Vec<String>,
}

/// The errors of a failed build, without cargo's closing "could not compile" and "aborting" lines.
pub(crate) fn errors(text: &str) -> Vec<CompileError> {
    let mut found: Vec<CompileError> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        if let Some(message) = line.strip_prefix("error") {
            let (code, message) = match message.strip_prefix('[').and_then(|m| m.split_once("]: "))
            {
                Some((code, message)) => (Some(code.to_owned()), message),
                None => (None, message.strip_prefix(": ").unwrap_or(message)),
            };
            // Closing lines of cargo and of nextest's `cargo test --no-run` wrapper.
            if message.starts_with("could not compile")
                || message.starts_with("aborting due to")
                || (message.starts_with("command `") && message.contains("exited with code"))
            {
                open = false;
                continue;
            }
            found.push(CompileError {
                path: String::new(),
                code,
                items: message
                    .split('`')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_owned)
                    .collect(),
            });
            open = true;
        } else if open && let Some(location) = line.trim_start().strip_prefix("--> ") {
            if let Some(error) = found.last_mut() {
                let mut parts = location.trim().rsplitn(3, ':');
                let path = parts.nth(2).unwrap_or(location.trim());
                error.path = path.to_owned();
            }
            open = false;
        }
    }
    found
}

#[cfg(test)]
#[path = "../tests/compile.rs"]
mod tests;
