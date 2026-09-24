use super::preparation::{CreationRequest, PreparationPreference};
use super::{
    Entry, Job, JobStage, JobState, MAX_RETAINED_JOBS, Manager, PreparationMode, PreparationPlan,
};
use aoe_map::{MapPackage, MapRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

const MAX_JOURNAL_BYTES: u64 = 2 * 1024 * 1024;
const SCHEMA: u8 = 2;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u8,
    next_id: u64,
    #[serde(deserialize_with = "bounded_records")]
    jobs: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: u64,
    request: MapRequest,
    mode: PreparationMode,
    state: JobState,
    content_hash: Option<String>,
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    submission: Option<super::submission::Identity>,
}

fn bounded_records<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Record>, D::Error> {
    struct Records;
    impl<'de> serde::de::Visitor<'de> for Records {
        type Value = Vec<Record>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 128 job records")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_RETAINED_JOBS)
            {
                return Err(serde::de::Error::custom(
                    "job history count exceeds its bound",
                ));
            }
            let mut records = Vec::new();
            while let Some(record) = sequence.next_element()? {
                if records.len() == MAX_RETAINED_JOBS {
                    return Err(serde::de::Error::custom(
                        "job history count exceeds its bound",
                    ));
                }
                records.push(record);
            }
            Ok(records)
        }
    }
    deserializer.deserialize_seq(Records)
}

impl Manager {
    pub(crate) fn load(
        directory: Option<&Path>,
        packages: &BTreeMap<String, MapPackage>,
    ) -> Result<Self, String> {
        let Some(directory) = directory else {
            return Ok(Self::default());
        };
        let path = directory.join("jobs/history.json");
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("job history exceeds its byte bound".into());
        }
        let journal: Journal = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if !matches!(journal.schema, 1 | SCHEMA) || journal.jobs.len() > MAX_RETAINED_JOBS {
            return Err("invalid job history schema or count".into());
        }
        let mut manager = Self {
            next_id: journal.next_id,
            jobs: BTreeMap::new(),
        };
        let mut submission_keys = std::collections::BTreeSet::new();
        for record in journal.jobs {
            if let Some(identity) = &record.submission {
                identity.validate()?;
                if !submission_keys.insert(identity.key.clone()) {
                    return Err("duplicate submission key".into());
                }
            }
            if record.id >= journal.next_id
                || manager.jobs.contains_key(&record.id)
                || record
                    .error
                    .as_ref()
                    .is_some_and(|error| error.len() > 16 * 1024)
                || record.content_hash.as_ref().is_some_and(|hash| {
                    hash.len() != 64
                        || !hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            {
                return Err("invalid job history identifier or metadata".into());
            }
            let normalized = record
                .request
                .normalized()
                .map_err(|error| error.to_string())?;
            if normalized != record.request {
                return Err("job history request is not canonical".into());
            }
            let preparation = PreparationPlan::resolve(
                CreationRequest {
                    request: normalized,
                    preparation: match record.mode {
                        PreparationMode::ProceduralFallback => PreparationPreference::Automatic,
                        PreparationMode::Overview => PreparationPreference::Overview,
                        PreparationMode::Detailed => PreparationPreference::Detailed,
                    },
                },
                record.mode != PreparationMode::ProceduralFallback,
            )?;
            let (state, error, hash) = match record.state {
                JobState::Completed => {
                    let hash = record.content_hash.ok_or("completed job has no package identity")?;
                    if packages.get(&hash).is_some_and(|package| package.request == normalized) {
                        (JobState::Completed, None, Some(hash))
                    } else {
                        (JobState::Failed, Some("Saved package is unavailable; retry this request.".into()), None)
                    }
                }
                JobState::Cancelled | JobState::CancelRequested => (JobState::Cancelled, None, None),
                JobState::Failed => (JobState::Failed, record.error, None),
                JobState::Queued | JobState::Running => (JobState::Failed, Some("Server restarted before this job finished; retry this request. Verified source cache entries can be reused.".into()), None),
            };
            let stage = match state {
                JobState::Completed => JobStage::Completed,
                JobState::Cancelled => JobStage::Cancelled,
                _ => JobStage::Failed,
            };
            manager.jobs.insert(
                record.id,
                Entry {
                    job: Job {
                        id: record.id,
                        request: normalized,
                        estimate: normalized.estimate().map_err(|error| error.to_string())?,
                        preparation,
                        state,
                        stage,
                        percent: (state == JobState::Completed).then_some(100),
                        progress: None,
                        eta_seconds: (state == JobState::Completed).then_some(0),
                        content_hash: hash,
                        error,
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    progress: crate::map_worker::progress::State::default(),
                    submission: record.submission,
                },
            );
        }
        Ok(manager)
    }
}

pub(super) async fn persist(directory: Option<&Path>, manager: &Manager) -> Result<(), String> {
    let Some(directory) = directory else {
        return Ok(());
    };
    let journal = Journal {
        schema: SCHEMA,
        next_id: manager.next_id,
        jobs: manager
            .jobs
            .values()
            .map(|entry| Record {
                id: entry.job.id,
                request: entry.job.request,
                mode: entry.job.preparation.mode,
                state: entry.job.state,
                content_hash: entry.job.content_hash.clone(),
                error: entry.job.error.clone(),
                submission: entry.submission.clone(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&journal).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err("map job history exceeds its byte bound".into());
    }
    let directory = directory.join("jobs");
    tokio::task::spawn_blocking(move || {
        fs::create_dir_all(&directory)?;
        let temporary = directory.join("history.tmp");
        // The job-manager mutex serializes the single owning server process.
        // A fixed staging name bounds and recovers a previous crash leftover.
        match fs::remove_file(&temporary) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
        let result = (|| {
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, directory.join("history.json"))?;
            // Rename is committed. A directory-sync failure must not roll back
            // the live manager while leaving the new history visible on disk.
            if let Err(error) = fs::File::open(&directory).and_then(|file| file.sync_all()) {
                tracing::warn!(%error, "map job history directory sync failed; crash durability uncertain");
            }
            Ok(())
        })();
        let _ = fs::remove_file(temporary);
        result
    }).await.map_err(|error| format!("map job storage task failed: {error}"))?
      .map_err(|error: std::io::Error| format!("map job storage failed: {error}"))
}

#[cfg(test)]
mod tests;
