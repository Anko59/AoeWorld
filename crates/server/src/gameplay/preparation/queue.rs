//! Two bounded lanes sharing the same off-tick workers. A worker chooses the
//! human camera lane when it becomes available, rather than reserving a FIFO
//! worker position for every speculative request ahead of the camera.
use super::*;
use std::collections::VecDeque;
use tokio::sync::{OwnedSemaphorePermit, oneshot};

type Job = Box<dyn FnOnce(bool) + Send>;

#[derive(Default)]
pub(super) struct Jobs {
    camera: VecDeque<Job>,
    background: VecDeque<Job>,
}

impl Preparation {
    pub(super) fn submit<T: Send + 'static>(
        &self,
        human: bool,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<oneshot::Receiver<T>, ()> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(());
        }
        // Background/AI work cannot consume the camera's admission reserve.
        let slots = if human {
            &self.viewport_slots
        } else {
            &self.slots
        };
        let slot: OwnedSemaphorePermit = slots.clone().try_acquire_owned().map_err(|_| ())?;
        let (tx, rx) = oneshot::channel();
        let cancelled = self.cancelled.clone();
        let queued = std::time::Instant::now();
        let job: Job = Box::new(move |active| {
            let _slot = slot;
            if active && !cancelled.load(Ordering::Acquire) {
                let started = std::time::Instant::now();
                let _ = tx.send(work());
                if started.elapsed() > std::time::Duration::from_millis(200) {
                    tracing::info!(
                        human_viewport = human,
                        queue_us = started.duration_since(queued).as_micros() as u64,
                        work_us = started.elapsed().as_micros() as u64,
                        "terrain preparation timing"
                    );
                }
            }
        });
        let mut jobs = self.jobs.lock().map_err(|_| ())?;
        if human {
            jobs.camera.push_back(job);
        } else {
            jobs.background.push_back(job);
        }
        drop(jobs);
        let (workers, jobs) = (self.workers.clone(), self.jobs.clone());
        tokio::spawn(async move {
            let worker = workers.acquire_owned().await.ok();
            let job = jobs.lock().ok().and_then(|mut jobs| {
                jobs.camera
                    .pop_front()
                    .or_else(|| jobs.background.pop_front())
            });
            if let Some(job) = job {
                let active = worker.is_some();
                if let Err(error) = tokio::task::spawn_blocking(move || job(active)).await {
                    tracing::warn!(%error, "terrain preparation worker failed");
                }
            }
        });
        Ok(rx)
    }

    pub(super) async fn terrain<T: Send + 'static>(
        &self,
        human: bool,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, ()> {
        self.submit(human, work)?.await.map_err(|_| ())
    }
}

impl super::super::GameplayService {
    pub(crate) async fn prepare_terrain<T: Send + 'static>(
        &self,
        human: bool,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, ()> {
        self.preparation.terrain(human, work).await
    }
}
