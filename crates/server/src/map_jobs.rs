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

const MAX_QUEUED_JOBS: usize = 2;

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
        job.stage = JobStage::BuildingFallbackPackage;
        job.percent = 5;
        job.eta_seconds = Some(1);
        Some(id)
    }

    fn enqueue(
        &mut self,
        request: MapRequest,
        estimate: MapEstimate,
    ) -> Result<(Job, Option<u64>), String> {
        if self.queued() >= MAX_QUEUED_JOBS && self.active() {
            return Err("map creation queue is full; wait for a job to finish".to_owned());
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let job = Job {
            id,
            request,
            estimate,
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

pub(super) async fn start(state: &AppState, request: MapRequest) -> Result<Job, String> {
    let request = request.normalized().map_err(|error| error.to_string())?;
    let estimate = request.estimate().map_err(|error| error.to_string())?;
    let (job, start) = {
        let mut manager = state.map_jobs.lock().await;
        manager.enqueue(request, estimate)?
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
        let Some((request, cancelled)) = active_input(&state, id).await else {
            return;
        };
        let directory = state.map_package_directory.clone();
        let worker = state.map_worker.clone();
        let cache = state.geodata_cache_directory.clone();
        if worker.is_some() {
            set_stage(&state, id, JobStage::PreparingOverview).await;
        }
        let result = tokio::task::spawn_blocking(move || {
            if cancelled.load(Ordering::SeqCst) {
                return Err("map creation cancelled".to_owned());
            }
            let package = if let Some(worker) = worker {
                let directory = directory.as_deref().ok_or_else(|| {
                    "source-backed map creation requires a configured package directory".to_owned()
                })?;
                let (package, elevation_pages, water_pages, vegetation_pages, land_use_pages) =
                    map_worker::prepare_overview(&worker, &cache, request, &cancelled)?;
                map_store::persist_prepared(
                    Some(directory),
                    &package,
                    &elevation_pages,
                    &water_pages,
                    &vegetation_pages,
                    &land_use_pages,
                )
                .map_err(|error| error.to_string())?;
                package
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

async fn active_input(state: &AppState, id: u64) -> Option<(MapRequest, Arc<AtomicBool>)> {
    let manager = state.map_jobs.lock().await;
    let entry = manager.jobs.get(&id)?;
    matches!(
        entry.job.state,
        JobState::Running | JobState::CancelRequested
    )
    .then(|| (entry.job.request, entry.cancelled.clone()))
}

async fn finish(state: &AppState, id: u64, result: Result<MapPackage, String>) {
    let (package, next) = {
        let mut manager = state.map_jobs.lock().await;
        let Some(entry) = manager.jobs.get_mut(&id) else {
            return;
        };
        let package = if entry.cancelled.load(Ordering::SeqCst) {
            entry.job.state = JobState::Cancelled;
            entry.job.stage = JobStage::Cancelled;
            entry.job.eta_seconds = None;
            None
        } else {
            match result {
                Ok(package) => {
                    entry.job.state = JobState::Completed;
                    entry.job.stage = JobStage::Completed;
                    entry.job.percent = 100;
                    entry.job.eta_seconds = Some(0);
                    entry.job.content_hash = Some(package.content_hash_hex());
                    Some(package)
                }
                Err(error) => {
                    entry.job.state = JobState::Failed;
                    entry.job.stage = JobStage::Failed;
                    entry.job.eta_seconds = None;
                    entry.job.error = Some(error);
                    None
                }
            }
        };
        (package, manager.start_next())
    };
    if let Some(package) = package {
        state
            .map_packages
            .write()
            .await
            .insert(package.content_hash_hex(), package);
    }
    if let Some(next) = next {
        launch(state.clone(), next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_limits_waiting_work_and_starts_in_request_order() {
        let request = MapRequest::default();
        let estimate = request.estimate().expect("estimate");
        let mut manager = Manager::default();
        let (first, start) = manager.enqueue(request, estimate).expect("first job");
        assert_eq!(start, Some(first.id));
        assert!(manager.active());
        manager.enqueue(request, estimate).expect("second job");
        manager.enqueue(request, estimate).expect("third job");
        assert!(manager.enqueue(request, estimate).is_err());
        assert_eq!(manager.queued(), 2);
        assert_eq!(first.stage, JobStage::BuildingFallbackPackage);
        assert_eq!(first.percent, 5);
        manager.jobs.get_mut(&first.id).expect("job").job.state = JobState::Completed;
        assert_eq!(manager.start_next(), Some(1));
        assert_eq!(
            manager.jobs.get(&1).expect("second job").job.stage,
            JobStage::BuildingFallbackPackage
        );
    }
}
