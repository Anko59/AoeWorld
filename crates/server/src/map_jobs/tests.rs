use super::*;
use std::{future::Future, task::Poll};

fn state_with_job() -> (AppState, u64, MapPackage) {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = AppState::new(&config, "publication-test").expect("state");
    let request = MapRequest::default();
    let (job, _) = state
        .map_jobs
        .try_lock()
        .expect("manager")
        .enqueue(
            request,
            request.estimate().expect("estimate"),
            PreparationPlan::fallback(),
        )
        .expect("enqueue");
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    (state, job.id, package)
}

#[tokio::test]
async fn completion_waits_for_publication_and_package_is_immediately_readable() {
    let (state, id, package) = state_with_job();
    let hash = package.content_hash_hex();
    let registry = state.map_packages.write().await;
    let mut pending = Box::pin(finish(&state, id, Ok(package)));
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    let waiting = status(&state, id).await.expect("job");
    assert_eq!(waiting.state, JobState::Running);
    assert_eq!(waiting.content_hash, None);
    drop(registry);
    pending.await;
    let completed = status(&state, id).await.expect("completed");
    assert_eq!(completed.state, JobState::Completed);
    assert_eq!(completed.content_hash.as_deref(), Some(hash.as_str()));
    assert!(
        crate::maps::package(axum::extract::Path(hash), axum::extract::State(state))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn cancellation_during_publication_wait_prevents_success_and_registration() {
    let (state, id, package) = state_with_job();
    let registry = state.map_packages.write().await;
    let mut pending = Box::pin(finish(&state, id, Ok(package)));
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    assert_eq!(
        cancel(&state, id).await.expect("job").state,
        JobState::CancelRequested
    );
    drop(registry);
    pending.await;
    assert_eq!(
        status(&state, id).await.expect("job").state,
        JobState::Cancelled
    );
    assert!(state.map_packages.read().await.is_empty());
}

#[test]
fn manager_limits_waiting_work_and_starts_in_request_order() {
    let request = MapRequest::default();
    let estimate = request.estimate().expect("estimate");
    let mut manager = Manager::default();
    let (first, start) = manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("first job");
    assert_eq!(start, Some(first.id));
    assert!(manager.active());
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("second job");
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("third job");
    assert!(
        manager
            .enqueue(request, estimate, PreparationPlan::fallback())
            .is_err()
    );
    assert_eq!(manager.queued(), 2);
    assert_eq!(first.stage, JobStage::BuildingFallbackPackage);
    assert_eq!(first.percent, 5);
    assert_eq!(first.eta_seconds, None);
    manager.jobs.get_mut(&first.id).expect("job").job.state = JobState::Completed;
    assert_eq!(manager.start_next(), Some(1));
    assert_eq!(
        manager.jobs.get(&1).expect("second job").job.stage,
        JobStage::BuildingFallbackPackage
    );
}

#[test]
fn completed_history_is_bounded_and_oldest_terminal_jobs_are_evicted() {
    let request = MapRequest::default();
    let estimate = request.estimate().expect("estimate");
    let mut manager = Manager::default();
    for id in 0..1_000_u64 {
        let (job, start) = manager
            .enqueue(request, estimate, PreparationPlan::fallback())
            .expect("job");
        assert_eq!(job.id, id);
        assert_eq!(start, Some(id));
        manager.jobs.get_mut(&id).expect("entry").job.state = match id % 3 {
            0 => JobState::Completed,
            1 => JobState::Cancelled,
            _ => JobState::Failed,
        };
        assert!(manager.jobs.len() <= MAX_RETAINED_JOBS);
    }
    assert_eq!(manager.jobs.len(), MAX_RETAINED_JOBS);
    assert_eq!(manager.jobs.first_key_value().map(|(id, _)| *id), Some(872));
}

#[test]
fn retirement_preserves_running_cancellation_and_queued_work() {
    let request = MapRequest::default();
    let estimate = request.estimate().expect("estimate");
    let mut manager = Manager::default();
    for _ in 0..MAX_RETAINED_JOBS - 1 {
        let (job, _) = manager
            .enqueue(request, estimate, PreparationPlan::fallback())
            .expect("job");
        manager.jobs.get_mut(&job.id).expect("entry").job.state = JobState::Completed;
    }
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("running");
    manager.jobs.get_mut(&127).expect("running").job.state = JobState::CancelRequested;
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("queued first");
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("queued second");
    assert_eq!(manager.jobs.len(), MAX_RETAINED_JOBS);
    assert_eq!(manager.jobs[&127].job.state, JobState::CancelRequested);
    assert_eq!(manager.jobs[&128].job.state, JobState::Queued);
    assert_eq!(manager.jobs[&129].job.state, JobState::Queued);
    assert!(!manager.jobs.contains_key(&0));
    assert!(!manager.jobs.contains_key(&1));
    assert!(
        manager
            .enqueue(request, estimate, PreparationPlan::fallback())
            .is_err()
    );
}

#[test]
fn identifier_exhaustion_does_not_overwrite_or_retire_jobs() {
    let request = MapRequest::default();
    let estimate = request.estimate().expect("estimate");
    let mut manager = Manager::default();
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .expect("first");
    manager.next_id = u64::MAX;
    assert!(
        manager
            .enqueue(request, estimate, PreparationPlan::fallback())
            .is_err()
    );
    assert_eq!(manager.next_id, u64::MAX);
    assert_eq!(manager.jobs.len(), 1);
    assert_eq!(manager.jobs[&0].job.state, JobState::Running);
}
