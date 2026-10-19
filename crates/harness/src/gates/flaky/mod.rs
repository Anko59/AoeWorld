//! Expiring, explicit browser-test quarantine. Never skips a test.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub test: String,
    pub gate: String,
    pub issue: String,
    added: String,
    reason: String,
}

pub(crate) struct Policy(Vec<Entry>);

impl Policy {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let today = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() / 86400;
        Self::parse_at(
            &fs::read(root.join("gates/flaky.json"))?,
            i64::try_from(today)?,
        )
    }

    fn parse_at(bytes: &[u8], today: i64) -> Result<Self> {
        let entries: Vec<Entry> = serde_json::from_slice(bytes)?;
        let mut keys = BTreeSet::new();
        for entry in &entries {
            let valid_name = !entry.test.is_empty()
                && entry.test.len() <= 512
                && entry
                    .test
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_:-./".contains(&b))
                && !entry.test.starts_with('/')
                && !entry
                    .test
                    .split('/')
                    .any(|part| part == ".." || part.is_empty());
            let valid_issue = entry.issue.strip_prefix('#').is_some_and(|number| {
                !number.is_empty()
                    && number.bytes().all(|b| b.is_ascii_digit())
                    && number.parse::<u64>().is_ok_and(|n| n > 0)
            });
            if !valid_name
                || !matches!(entry.gate.as_str(), "test-wasm" | "test-e2e")
                || !valid_issue
                || entry.reason.trim().is_empty()
                || entry.reason.len() > 1000
                || !keys.insert((&entry.gate, &entry.test))
            {
                return Err(format!(
                    "invalid or duplicate quarantine entry: {} ({})",
                    entry.test, entry.issue
                )
                .into());
            }
            let age = today - date(&entry.added)?;
            if !(0..=14).contains(&age) {
                return Err(format!("quarantine expired or future-dated: {} ({}, added {}, age {age} days; maximum 14)", entry.test, entry.issue, entry.added).into());
            }
        }
        Ok(Self(entries))
    }

    pub(crate) fn find(&self, gate: &str, test: &str) -> Option<&Entry> {
        self.0
            .iter()
            .find(|entry| entry.gate == gate && entry.test == test)
    }
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Strict UTC calendar date, expressed as days since the Unix epoch.
fn date(text: &str) -> Result<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return Err(format!("invalid quarantine date: {text}").into());
    }
    let year: i64 = text[..4].parse()?;
    let month: usize = text[5..7].parse()?;
    let day: i64 = text[8..].parse()?;
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year < 1970 || !(1..=12).contains(&month) || day < 1 || day > months[month - 1] {
        return Err(format!("invalid quarantine date: {text}").into());
    }
    Ok((1970..year)
        .map(|y| if leap(y) { 366 } else { 365 })
        .sum::<i64>()
        + months[..month - 1].iter().sum::<i64>()
        + day
        - 1)
}

#[derive(Serialize)]
pub(crate) struct Pass<'a> {
    test: &'a str,
    gate: &'a str,
    issue: &'a str,
    attempt: u8,
}

/// Fresh per-run JSON lines survive a later failure of another test.
pub(crate) struct Journal(fs::File);

impl Journal {
    pub(crate) fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self(fs::File::create(path)?))
    }

    fn record(&mut self, pass: &Pass<'_>) -> Result<()> {
        serde_json::to_writer(&mut self.0, pass)?;
        writeln!(self.0)?;
        self.0.flush()?;
        Ok(())
    }
}

fn message(test: &str, attempt: u8, issue: &str) -> String {
    format!("flaky: {test} passed on attempt {attempt} (quarantined, {issue})")
}

/// The caller has already observed attempt 1 failing. Only this entry can retry.
pub(crate) fn retry(
    entry: Option<&Entry>,
    test: &str,
    journal: &mut Journal,
    mut rerun: impl FnMut() -> Result<bool>,
) -> Result<bool> {
    let Some(entry) = entry else { return Ok(false) };
    for attempt in 2..=3 {
        if rerun()? {
            journal.record(&Pass {
                test,
                gate: &entry.gate,
                issue: &entry.issue,
                attempt,
            })?;
            println!("{}", message(test, attempt, &entry.issue));
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;
