use crate::{AppState, map_store, map_worker};
use aoe_map::{MAP_SCHEMA_VERSION, MapEstimate, MapPackage, MapRequest};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod preparation;
pub(super) use preparation::{CreationRequest, PreparationMode, PreparationPlan};

const MAX_QUEUED_JOBS: usize = 2;
const MAX_RETAINED_JOBS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum JobState {
    Queued,
    Running,
    CancelRequested,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum JobStage {
    Queued,
    BuildingFallbackPackage,
    PreparingOverview,
    PreparingDetailed,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Job {
    pub id: u64,
    pub request: MapRequest,
    pub estimate: MapEstimate,
    pub preparation: PreparationPlan,
    pub state: JobState,
    pub stage: JobStage,
    pub percent: u8,
    pub eta_seconds: Option<u64>,
    pub content_hash: Option<String>,
    pub error: Option<String>,
}

struct Entry {
    job: Job,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct Manager {
    next_id: u64,
    jobs: BTreeMap<u64, Entry>,
}

impl Manager {
    fn reserve_history_slot(&mut self) -> Result<(), String> {
        if self.jobs.len() < MAX_RETAINED_JOBS {
            return Ok(());
        }
        let retired = self.jobs.iter().find_map(|(id, entry)| {
            matches!(
                entry.job.state,
                JobState::Completed | JobState::Cancelled | JobState::Failed
            )
            .then_some(*id)
        });
        let Some(id) = retired else {
            return Err("map job history is full of active work".to_owned());
        };
        self.jobs.remove(&id);
        Ok(())
    }

    fn active(&self) -> bool {
        self.jobs.values().any(|entry| {
            matches!(
                entry.job.state,
                JobState::Running | JobState::CancelRequested
            )
        })
    }

    fn queued(&self) -> usize {
        self.jobs
            .values()
            .filter(|entry| entry.job.state == JobState::Queued)
            .count()
    }

    fn start_next(&mut self) -> Option<u64> {
        if self.active() {
            return None;
        }
        let id = self
            .jobs
            .iter()
            .find(|(_, entry)| entry.job.state == JobState::Queued)
            .map(|(id, _)| *id)?;
        let job = &mut self.jobs.get_mut(&id)?.job;
        job.state = JobState::Running;
        job.stage = match job.preparation.mode {
            PreparationMode::ProceduralFallback => JobStage::BuildingFallbackPackage,
            PreparationMode::Overview => JobStage::PreparingOverview,
            PreparationMode::Detailed => JobStage::PreparingDetailed,
        };
        job.percent = 5;
        job.eta_seconds = None;
        Some(id)
    }

    fn enqueue(
        &mut self,
        request: MapRequest,
        estimate: MapEstimate,
        preparation: PreparationPlan,
    ) -> Result<(Job, Option<u64>), String> {
        if self.queued() >= MAX_QUEUED_JOBS && self.active() {
            return Err("map creation queue is full; wait for a job to finish".to_owned());
        }
        let id = self.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| "map job identifiers are exhausted; restart the server".to_owned())?;
        self.reserve_history_slot()?;
        self.next_id = next_id;
        let job = Job {
            id,
            request,
            estimate,
            preparation,
            state: JobState::Queued,
            stage: JobStage::Queued,
            percent: 0,
            eta_seconds: None,
            content_hash: None,
            error: None,
        };
        self.jobs.insert(
            id,
            Entry {
                job,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        let start = self.start_next();
        let job = self
            .jobs
            .get(&id)
            .map(|entry| entry.job.clone())
            .ok_or_else(|| "map job disappeared after insertion".to_owned())?;
        Ok((job, start))
    }
}

pub(super) async fn start(state: &AppState, input: CreationRequest) -> Result<Job, String> {
    let preparation = PreparationPlan::resolve(input, state.map_worker.is_some())?;
    let request = input
        .request
        .normalized()
        .map_err(|error| error.to_string())?;
    let estimate = request.estimate().map_err(|error| error.to_string())?;
    let (job, start) = {
        let mut manager = state.map_jobs.lock().await;
        manager.enqueue(request, estimate, preparation)?
    };
    if let Some(id) = start {
        launch(state.clone(), id);
    }
    Ok(job)
}

pub(super) async fn status(state: &AppState, id: u64) -> Option<Job> {
    state
        .map_jobs
        .lock()
        .await
        .jobs
        .get(&id)
        .map(|entry| entry.job.clone())
}

pub(super) async fn list(state: &AppState) -> Vec<Job> {
    state
        .map_jobs
        .lock()
        .await
        .jobs
        .values()
        .map(|entry| entry.job.clone())
        .collect()
}

pub(super) async fn cancel(state: &AppState, id: u64) -> Option<Job> {
    let mut manager = state.map_jobs.lock().await;
    let entry = manager.jobs.get_mut(&id)?;
    match entry.job.state {
        JobState::Queued => {
            entry.job.state = JobState::Cancelled;
            entry.job.stage = JobStage::Cancelled;
            entry.job.eta_seconds = None;
        }
        JobState::Running => {
            entry.cancelled.store(true, Ordering::SeqCst);
            entry.job.state = JobState::CancelRequested;
            entry.job.stage = JobStage::Cancelling;
            entry.job.eta_seconds = None;
        }
        JobState::CancelRequested
        | JobState::Completed
        | JobState::Cancelled
        | JobState::Failed => {}
    }
    Some(entry.job.clone())
}

fn launch(state: AppState, id: u64) {
    tokio::spawn(async move {
        let Some((request, preparation, cancelled)) = active_input(&state, id).await else {
            return;
        };
        let directory = state.map_package_directory.clone();
        let worker = state.map_worker.clone();
        let cache = state.geodata_cache_directory.clone();
        if worker.is_some() {
            let stage = match preparation.mode {
                PreparationMode::Detailed => JobStage::PreparingDetailed,
                _ => JobStage::PreparingOverview,
            };
            set_stage(&state, id, stage).await;
        }
        let result = tokio::task::spawn_blocking(move || {
            if cancelled.load(Ordering::SeqCst) {
                return Err("map creation cancelled".to_owned());
            }
            let package = if let Some(worker) = worker {
                let directory = directory.as_deref().ok_or_else(|| {
                    "source-backed map creation requires a configured package directory".to_owned()
                })?;
                let package = map_worker::prepare(
                    &worker,
                    &cache,
                    directory,
                    request,
                    preparation,
                    &cancelled,
                )?;
                map_store::verify_stored(directory, &package).map_err(|error| error.to_string())?
            } else {
                let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new())
                    .map_err(|error| error.to_string())?;
                map_store::persist(directory.as_deref(), &package)
                    .map_err(|error| error.to_string())?;
                package
            };
            if cancelled.load(Ordering::SeqCst) {
                return Err("map creation cancelled".to_owned());
            }
            Ok::<_, String>(package)
        })
        .await
        .unwrap_or_else(|_| Err("map creation task failed".to_owned()));
        finish(&state, id, result).await;
    });
}

async fn set_stage(state: &AppState, id: u64, stage: JobStage) {
    let mut manager = state.map_jobs.lock().await;
    if let Some(entry) = manager.jobs.get_mut(&id)
        && entry.job.state == JobState::Running
    {
        entry.job.stage = stage;
        entry.job.percent = 10;
        entry.job.eta_seconds = None;
    }
}

async fn active_input(
    state: &AppState,
    id: u64,
) -> Option<(MapRequest, PreparationPlan, Arc<AtomicBool>)> {
    let manager = state.map_jobs.lock().await;
    let entry = manager.jobs.get(&id)?;
    matches!(
        entry.job.state,
        JobState::Running | JobState::CancelRequested
    )
    .then(|| {
        (
            entry.job.request,
            entry.job.preparation,
            entry.cancelled.clone(),
        )
    })
}

async fn finish(state: &AppState, id: u64, result: Result<MapPackage, String>) {
    // Acquire the registry first so status and cancellation remain responsive
    // while publication waits. No completion is observable before insertion.
    let publication = match result {
        Ok(package) => Ok((state.map_packages.write().await, package)),
        Err(error) => Err(error),
    };
    let next = {
        let mut manager = state.map_jobs.lock().await;
        let Some(entry) = manager.jobs.get_mut(&id) else {
            return;
        };
        if entry.cancelled.load(Ordering::SeqCst) {
            entry.job.state = JobState::Cancelled;
            entry.job.stage = JobStage::Cancelled;
            entry.job.eta_seconds = None;
        } else {
            match publication {
                Ok((mut packages, package)) => {
                    let hash = package.content_hash_hex();
                    packages.insert(hash.clone(), package);
                    entry.job.state = JobState::Completed;
                    entry.job.stage = JobStage::Completed;
                    entry.job.percent = 100;
                    entry.job.eta_seconds = Some(0);
                    entry.job.content_hash = Some(hash);
                }
                Err(error) => {
                    entry.job.state = JobState::Failed;
                    entry.job.stage = JobStage::Failed;
                    entry.job.eta_seconds = None;
                    entry.job.error = Some(error);
                }
            }
        };
        manager.start_next()
    };
    if let Some(next) = next {
        launch(state.clone(), next);
    }
}

#[cfg(test)]
mod tests;
