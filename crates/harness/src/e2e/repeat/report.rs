//! The per-test tally of a repeated run, read from Playwright's JSON report.
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One test on one project, over every attempt of the run.
#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Tally {
    pub file: String,
    pub line: u64,
    pub project: String,
    pub title: String,
    pub passed: u32,
    /// Failed, timed out or interrupted attempts.
    pub failed: u32,
    pub skipped: u32,
    /// One character per attempt, in start order: `.` passed, `F` failed, `T` timed out,
    /// `I` interrupted, `S` skipped.
    pub sequence: String,
    /// The first error line of each bad attempt, numbered from 1 in start order.
    pub errors: Vec<String>,
}

impl Tally {
    fn attempts(&self) -> u32 {
        self.passed + self.failed + self.skipped
    }

    /// The consecutive passes that end the run.
    fn closing_passes(&self) -> usize {
        self.sequence
            .chars()
            .rev()
            .take_while(|&mark| mark == '.')
            .count()
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Summary {
    pub repeat: u32,
    pub revision: Option<String>,
    pub tests: Vec<Tally>,
}

impl Summary {
    pub(super) fn passed(&self) -> u32 {
        self.tests.iter().map(|test| test.passed).sum()
    }

    pub(super) fn failed(&self) -> u32 {
        self.tests.iter().map(|test| test.failed).sum()
    }

    /// Why the run proves nothing, if it does not: no test ran, or a test has fewer
    /// attempts than the run asked for (a worker died, the run was cut short).
    pub(super) fn incomplete(&self) -> Option<String> {
        if self.tests.iter().all(|test| test.passed + test.failed == 0) {
            return Some("no test ran: check that the spec runs on the project".into());
        }
        self.tests.iter().find_map(|test| {
            (test.attempts() != self.repeat).then(|| {
                format!(
                    "{} [{}] has {} attempts, expected {}",
                    test.title,
                    test.project,
                    test.attempts(),
                    self.repeat
                )
            })
        })
    }

    pub(super) fn markdown(&self) -> String {
        let revision = self.revision.as_deref().unwrap_or("unknown revision");
        let mut text = format!(
            "### e2e repeat: {} runs per test at {revision}\n\nPassed {}, failed {}.\n\n\
             | test | project | passed | failed | closing passes | sequence |\n\
             | --- | --- | ---: | ---: | ---: | --- |\n",
            self.repeat,
            self.passed(),
            self.failed()
        );
        for test in &self.tests {
            text.push_str(&format!(
                "| `{}:{}` {} | {} | {} | {} | {} | `{}` |\n",
                test.file,
                test.line,
                test.title.replace('|', "\\|"),
                test.project,
                test.passed,
                test.failed,
                test.closing_passes(),
                test.sequence
            ));
        }
        for test in self.tests.iter().filter(|test| !test.errors.is_empty()) {
            text.push_str(&format!(
                "\n`{}:{}` [{}] failures:\n",
                test.file, test.line, test.project
            ));
            for error in &test.errors {
                text.push_str(&format!("- {}\n", error.replace('|', "\\|")));
            }
        }
        text
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Playwright report missing {key}"))
}

fn first_line(attempt: &Value) -> String {
    let error = attempt
        .get("error")
        .filter(|error| !error.is_null())
        .or_else(|| attempt.get("errors")?.as_array()?.first());
    let message = error
        .and_then(|error| {
            error["message"]
                .as_str()
                .or_else(|| error["value"].as_str())
        })
        .unwrap_or("no error message");
    let message = crate::ship::run::strip_ansi(message);
    let line = message.lines().find(|line| !line.trim().is_empty());
    let line = line.unwrap_or("no error message").trim();
    let mut short: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        short.push('…');
    }
    short
}

type Key = (String, u64, String, String);
/// Start time, mark and error line of each attempt.
type Attempts = Vec<(String, char, Option<String>)>;

fn collect(
    suite: &Value,
    parents: &[String],
    found: &mut BTreeMap<Key, Attempts>,
) -> Result<(), String> {
    for spec in suite
        .get("specs")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let mut titles = parents.to_vec();
        titles.push(text(spec, "title")?.to_owned());
        for test in spec
            .get("tests")
            .and_then(Value::as_array)
            .ok_or("Playwright report missing tests")?
        {
            let key = (
                text(spec, "file")?.to_owned(),
                spec.get("line").and_then(Value::as_u64).unwrap_or(0),
                text(test, "projectName")?.to_owned(),
                titles.join(" "),
            );
            let attempts = found.entry(key).or_default();
            for attempt in test
                .get("results")
                .and_then(Value::as_array)
                .ok_or("Playwright report missing results")?
            {
                let mark = match text(attempt, "status")? {
                    "passed" => '.',
                    "failed" => 'F',
                    "timedOut" => 'T',
                    "skipped" => 'S',
                    _ => 'I',
                };
                let error = (!matches!(mark, '.' | 'S')).then(|| first_line(attempt));
                let started = attempt["startTime"].as_str().unwrap_or_default();
                attempts.push((started.to_owned(), mark, error));
            }
        }
    }
    for child in suite
        .get("suites")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let mut titles = parents.to_vec();
        let title = text(child, "title")?;
        // The outer file suite is not part of the spec's title path.
        if !title.ends_with(".spec.ts") && !title.is_empty() {
            titles.push(title.to_owned());
        }
        collect(child, &titles, found)?;
    }
    Ok(())
}

/// Tallies the report. A report with global errors (a spec that does not load) is refused.
pub(super) fn summarize(
    report: &Value,
    repeat: u32,
    revision: Option<String>,
) -> Result<Summary, String> {
    if let Some(error) = report
        .get("errors")
        .and_then(Value::as_array)
        .and_then(|errors| errors.first())
    {
        return Err(format!(
            "Playwright reported an error before or outside the tests: {}",
            first_line(&serde_json::json!({ "error": error }))
        ));
    }
    let mut found = BTreeMap::new();
    for suite in report
        .get("suites")
        .and_then(Value::as_array)
        .ok_or("Playwright report missing suites")?
    {
        collect(suite, &[], &mut found)?;
    }
    let tests = found
        .into_iter()
        .map(|((file, line, project, title), mut attempts)| {
            attempts.sort_by(|a, b| a.0.cmp(&b.0));
            let count = |mark: char| attempts.iter().filter(|a| a.1 == mark).count() as u32;
            let skipped = count('S');
            let passed = count('.');
            Tally {
                file,
                line,
                project,
                title,
                passed,
                failed: attempts.len() as u32 - passed - skipped,
                skipped,
                sequence: attempts.iter().map(|attempt| attempt.1).collect(),
                errors: attempts
                    .iter()
                    .enumerate()
                    .filter_map(|(index, attempt)| {
                        Some(format!("run {}: {}", index + 1, attempt.2.as_ref()?))
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Summary {
        repeat,
        revision,
        tests,
    })
}
