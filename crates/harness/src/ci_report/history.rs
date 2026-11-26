//! The history of the committed baselines: when a row was redefined (its
//! baseline moved by more than two times between two commits, so its line
//! restarts there) and when an increase was accepted. Both come from the Git
//! history of `baselines/perf/micro.json`.
use super::format::{first_sentence, label, si};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub(super) struct Accepted {
    pub(super) dev: u64,
    pub(super) reason: String,
}

/// `baselines/perf/micro.json` as one commit left it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Version {
    /// Index of the commit in the `dev` log, oldest first.
    pub(super) position: usize,
    pub(super) at: String,
    pub(super) pr: Option<u64>,
    pub(super) title: String,
    pub(super) instructions: BTreeMap<String, u64>,
    pub(super) reference: BTreeMap<String, u64>,
    pub(super) accepted: BTreeMap<String, Accepted>,
}

fn counts(value: &Value, field: &str) -> BTreeMap<String, u64> {
    value[field]
        .as_object()
        .map(|rows| {
            rows.iter()
                .filter_map(|(name, count)| {
                    let row = name.strip_prefix("instructions::").unwrap_or(name);
                    Some((row.to_owned(), count.as_u64()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The counts, reference counts and accepted increases of a `micro.json`.
pub(super) fn parse_micro(text: &str) -> Result<Version> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("micro.json: {error}"))?;
    let instructions = counts(&value, "instructions");
    if instructions.is_empty() {
        return Err("micro.json: no instruction baselines".to_owned());
    }
    let accepted = value["accepted_increase"]
        .as_object()
        .map(|rows| {
            rows.iter()
                .filter_map(|(name, entry)| {
                    let row = name.strip_prefix("instructions::").unwrap_or(name);
                    let accepted = Accepted {
                        dev: entry["dev"].as_u64()?,
                        reason: entry["reason"].as_str().unwrap_or_default().to_owned(),
                    };
                    Some((row.to_owned(), accepted))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Version {
        instructions,
        reference: counts(&value, "instruction_reference"),
        accepted,
        ..Version::default()
    })
}

/// The optimized gzip size of a `wasm.json`.
pub(super) fn parse_wasm(text: &str) -> Result<u64> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("wasm.json: {error}"))?;
    value["optimized_gzip_bytes"]
        .as_u64()
        .ok_or_else(|| "wasm.json: no optimized_gzip_bytes".to_owned())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventKind {
    /// The row counts something else from here: its line restarts.
    Redefined,
    /// A higher count was accepted on purpose.
    Accepted,
}

/// A commit that changed what a row means or the level it is accepted at.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Event {
    pub(crate) id: String,
    pub(crate) kind: EventKind,
    pub(crate) rows: Vec<String>,
    pub(crate) position: usize,
    pub(crate) at: String,
    pub(crate) pr: Option<u64>,
    pub(crate) title: String,
    pub(crate) what: String,
}

/// `A`, `B`, … `Z`, `AA`, `AB`, …
fn letters(index: usize) -> String {
    let letter = |n: usize| char::from(b'A' + (n % 26) as u8);
    if index < 26 {
        letter(index).to_string()
    } else {
        format!("{}{}", letter(index / 26 - 1), letter(index))
    }
}

/// Redefinitions and accepted increases between consecutive versions
/// (oldest first), lettered in order. A row that is new in a version was not
/// redefined; a baseline that changed by exactly two times was not either.
pub(super) fn events(versions: &[Version]) -> Vec<Event> {
    let mut found: Vec<Event> = Vec::new();
    for pair in versions.windows(2) {
        let (old, new) = (&pair[0], &pair[1]);
        let mut moved = Vec::new();
        let mut notes = Vec::new();
        for (row, &now) in &new.instructions {
            let Some(&was) = old.instructions.get(row) else {
                continue;
            };
            if now > was.saturating_mul(2) || was > now.saturating_mul(2) {
                moved.push(row.clone());
                notes.push(format!(
                    "{} baseline {} to {}",
                    label(row),
                    si(was as f64),
                    si(now as f64)
                ));
            }
        }
        let mut accepted = Vec::new();
        let mut reasons = Vec::new();
        for (row, entry) in &new.accepted {
            if old.accepted.get(row).is_none_or(|was| was.dev != entry.dev) {
                accepted.push(row.clone());
                reasons.push(first_sentence(&entry.reason));
            }
        }
        for (kind, rows, mut notes) in [
            (EventKind::Redefined, moved, notes),
            (EventKind::Accepted, accepted, reasons),
        ] {
            if rows.is_empty() {
                continue;
            }
            notes.dedup();
            found.push(Event {
                id: letters(found.len()),
                kind,
                rows,
                position: new.position,
                at: new.at.clone(),
                pr: new.pr,
                title: new.title.clone(),
                what: notes.join("; "),
            });
        }
    }
    found
}
