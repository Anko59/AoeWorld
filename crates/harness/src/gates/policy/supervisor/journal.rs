//! Durable filesystem plumbing for model feedback, never an owned service lease.
mod io;
#[cfg(test)]
mod tests;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{error::Error, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_ENTRIES: usize = 32;
const MAX_BYTES: usize = 32768;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Phase {
    CreatedIntent,
    ModelObservation,
}
/// Deserialized history is untrusted correlation data, NOT an OwnedLease.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    sequence: u32,
    lease_id: String,
    phase: Phase,
    cid: Option<String>,
    identity: Value,
    previous_hash: String,
    entry_hash: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema: u32,
    authoritative: bool,
    mode: String,
    entries: Vec<Entry>,
}
fn identity() -> Value {
    json!(super::identity::ExpectedIdentity::simulation())
}
fn hash(entry: &Entry) -> Result<String> {
    let mut digest = blake3::Hasher::new();
    digest.update(b"aoeworld:model-lease-journal-entry:v1\0");
    digest.update(&serde_json::to_vec(&(
        entry.sequence,
        &entry.lease_id,
        &entry.phase,
        &entry.cid,
        &entry.identity,
        &entry.previous_hash,
    ))?);
    Ok(digest.finalize().to_hex().to_string())
}
impl Ledger {
    fn empty() -> Self {
        Self {
            schema: 1,
            authoritative: false,
            mode: "MODEL_ONLY".into(),
            entries: Vec::new(),
        }
    }
    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err("model journal exceeds32KiB; quarantine".into());
        }
        let ledger: Self = serde_json::from_slice(bytes)?;
        ledger.validate()?;
        Ok(ledger)
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.authoritative
            || self.mode != "MODEL_ONLY"
            || self.entries.len() > MAX_ENTRIES
        {
            return Err("invalid model journal schema/mode/bounds; quarantine".into());
        }
        let mut previous = "0".repeat(64);
        for (index, entry) in self.entries.iter().enumerate() {
            let intent = index.is_multiple_of(2);
            let expected_id = format!("model-lease-{:04}", index / 2 + 1);
            let expected_phase = if intent {
                Phase::CreatedIntent
            } else {
                Phase::ModelObservation
            };
            let expected_cid = if intent { None } else { Some("a".repeat(64)) };
            if entry.sequence != index as u32 + 1
                || entry.sequence > 256
                || entry.lease_id != expected_id
                || entry.phase != expected_phase
                || entry.cid != expected_cid
                || entry.identity != identity()
                || entry.previous_hash != previous
                || entry.entry_hash != hash(entry)?
            {
                return Err("inconsistent model journal sequence/identity/hash; quarantine".into());
            }
            previous.clone_from(&entry.entry_hash);
        }
        Ok(())
    }
    fn append(&mut self) -> Result<()> {
        if self.entries.len() >= MAX_ENTRIES {
            return Err("model journal entry budget exhausted".into());
        }
        let index = self.entries.len();
        let intent = index.is_multiple_of(2);
        let mut entry = Entry {
            sequence: index as u32 + 1,
            lease_id: format!("model-lease-{:04}", index / 2 + 1),
            phase: if intent {
                Phase::CreatedIntent
            } else {
                Phase::ModelObservation
            },
            cid: if intent { None } else { Some("a".repeat(64)) },
            identity: identity(),
            previous_hash: self
                .entries
                .last()
                .map_or_else(|| "0".repeat(64), |entry| entry.entry_hash.clone()),
            entry_hash: String::new(),
        };
        entry.entry_hash = hash(&entry)?;
        self.entries.push(entry);
        self.validate()
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BYTES {
            return Err("model journal serialization exceeds32KiB".into());
        }
        Ok(bytes)
    }
}
/// Appends fixed simulated correlation events using actual write/fsync/rename/fsync.
/// An incomplete model intent is completed on reuse; no daemon ownership is minted.
pub(super) fn persist_model(directory: &Path) -> Result<Value> {
    let store = io::Store::open(directory)?;
    let initial = store.read()?;
    let previous_endpoint = initial
        .as_ref()
        .map(|bytes| blake3::hash(bytes).to_hex().to_string());
    let mut ledger = match &initial {
        Some(bytes) => Ledger::parse(bytes)?,
        None => Ledger::empty(),
    };
    let resumed_intent = ledger.entries.len() % 2 == 1;
    let count = if resumed_intent { 1 } else { 2 };
    if ledger.entries.len() + count > MAX_ENTRIES {
        return Err("model journal entry budget exhausted".into());
    }
    let mut endpoint = initial;
    for _ in 0..count {
        ledger.append()?;
        let bytes = ledger.bytes()?;
        store.replace(endpoint.as_deref(), &bytes)?;
        endpoint = Some(bytes);
    }
    let bytes = store.read()?.ok_or("model journal endpoint missing")?;
    let reopened = Ledger::parse(&bytes)?;
    if Some(&bytes) != endpoint.as_ref() {
        return Err("model journal endpoint changed".into());
    }
    Ok(
        json!({"schema":1,"status":"MODEL_ONLY","authoritative":false,"observed_uid":store.uid(),"entries":reopened.entries.len(),"sequence":reopened.entries.last().map(|entry|entry.sequence),"endpoint_blake3":blake3::hash(&bytes).to_hex().to_string(),"previous_endpoint_blake3":previous_endpoint,"resumed_model_intent":resumed_intent,"fsync_calls_completed":true,
        "limits":["fixed simulated lease IDs/CID only, no Docker object observation or owned service lease","file and directory sync calls completed; not a power-loss/durable deployment qualification","local hashes detect inconsistency, not hostile same-user history rewrite or consistent rollback","same-user/reverted races, ACLs and filesystem wall-time remain unqualified","not connected to authenticated supervisor or daemon crash/SIGKILL recovery"]}),
    )
}
