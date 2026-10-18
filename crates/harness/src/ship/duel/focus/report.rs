//! The check report the duel saves as `check.json`, and the one way two reports are combined.
use super::compile::CompileError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A panic a test run printed: the file it was raised in (normalised), `path:line:col`, and the assertion lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Panic {
    pub(crate) path: String,
    pub(crate) location: String,
    pub(crate) message: String,
    /// The failing test whose output the panic follows, named like `CheckReport::failed`; none for a nameless abort.
    #[serde(default)]
    pub(crate) test: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct CheckReport {
    pub(crate) passed: bool,
    #[serde(default)]
    pub(crate) build_failed: bool,
    #[serde(default)]
    pub(crate) unavailable: Option<String>,
    pub(crate) tests: Vec<String>,
    pub(crate) failed: Vec<String>,
    pub(crate) untouched: Vec<String>,
    #[serde(default)]
    pub(crate) locations: BTreeMap<String, (String, usize)>,
    /// Panics of a failed run (a panic in a shared helper aborts the runner without a `FAIL` line).
    #[serde(default)]
    pub(crate) panics: Vec<Panic>,
    /// The compiler errors of a build that failed; none when its output was cut short.
    #[serde(default)]
    pub(crate) compile_errors: Vec<CompileError>,
}

impl CheckReport {
    /// Fold `other` (another runner's report) into this one. Every field is named in the
    /// destructuring, so a field added to the struct cannot compile until it is merged here.
    pub(crate) fn merge(&mut self, other: CheckReport) {
        let CheckReport {
            passed,
            build_failed,
            unavailable,
            tests,
            failed,
            untouched,
            locations,
            panics,
            compile_errors,
        } = other;
        self.passed &= passed;
        self.build_failed |= build_failed;
        self.unavailable = self.unavailable.take().or(unavailable);
        self.tests.extend(tests);
        self.failed.extend(failed);
        self.untouched.extend(untouched);
        self.locations.extend(locations);
        self.panics.extend(panics);
        self.compile_errors.extend(compile_errors);
    }

    /// The failing names, then each panic's location and assertion: what a failed check tells its reader.
    pub(crate) fn failure_text(&self) -> String {
        let mut what = self.failed.join(", ");
        let mut shown: Vec<(&str, &str)> = Vec::new();
        for panic in &self.panics {
            let key = (panic.location.as_str(), panic.message.as_str());
            if shown.contains(&key) {
                continue;
            }
            shown.push(key);
            what.push_str(&format!(
                "\npanicked at {}\n{}",
                panic.location, panic.message
            ));
        }
        what
    }
}
