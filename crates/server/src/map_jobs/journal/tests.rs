use super::*;

fn manager_with_jobs() -> Manager {
    let request = MapRequest::default();
    let estimate = request.estimate().unwrap();
    let mut manager = Manager::default();
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .unwrap();
    manager
        .enqueue(request, estimate, PreparationPlan::fallback())
        .unwrap();
    manager
}

#[tokio::test]
async fn restart_recovers_history_and_ids_without_restarting_interrupted_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = manager_with_jobs();
    manager.jobs.get_mut(&1).unwrap().job.state = JobState::CancelRequested;
    persist(Some(directory.path()), &manager).await.unwrap();
    let mut restored = Manager::load(Some(directory.path()), &BTreeMap::new()).unwrap();
    assert_eq!(restored.jobs[&0].job.state, JobState::Failed);
    assert!(
        restored.jobs[&0]
            .job
            .error
            .as_ref()
            .unwrap()
            .contains("restarted")
    );
    assert_eq!(restored.jobs[&1].job.state, JobState::Cancelled);
    assert!(!restored.active());
    assert_eq!(restored.queued(), 0);
    let (next, start) = restored
        .enqueue(
            MapRequest::default(),
            MapRequest::default().estimate().unwrap(),
            PreparationPlan::fallback(),
        )
        .unwrap();
    assert_eq!(next.id, 2);
    assert_eq!(start, Some(2));
}

#[tokio::test]
async fn completion_recovers_only_with_matching_saved_package_and_staging_is_reused() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = manager_with_jobs();
    let package = MapPackage::new(
        aoe_map::MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
    )
    .unwrap();
    let hash = package.content_hash_hex();
    manager.jobs.get_mut(&0).unwrap().job.state = JobState::Completed;
    manager.jobs.get_mut(&0).unwrap().job.content_hash = Some(hash.clone());
    persist(Some(directory.path()), &manager).await.unwrap();
    std::fs::write(directory.path().join("jobs/history.tmp"), b"crash leftover").unwrap();
    persist(Some(directory.path()), &manager).await.unwrap();
    assert!(!directory.path().join("jobs/history.tmp").exists());
    let packages = BTreeMap::from([(hash.clone(), package)]);
    let restored = Manager::load(Some(directory.path()), &packages).unwrap();
    assert_eq!(restored.jobs[&0].job.state, JobState::Completed);
    assert_eq!(restored.jobs[&0].job.content_hash.as_ref(), Some(&hash));
    let missing = Manager::load(Some(directory.path()), &BTreeMap::new()).unwrap();
    assert_eq!(missing.jobs[&0].job.state, JobState::Failed);
}

#[tokio::test]
async fn malformed_duplicate_noncanonical_and_oversized_history_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    persist(Some(directory.path()), &manager_with_jobs())
        .await
        .unwrap();
    let path = directory.path().join("jobs/history.json");
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for kind in 0..7 {
        let mut value = original.clone();
        match kind {
            0 => value["schema"] = 99.into(),
            1 => value["jobs"][1]["id"] = 0.into(),
            2 => value["next_id"] = 1.into(),
            3 => value["jobs"][0]["content_hash"] = "invalid".into(),
            4 => value["jobs"][0]["error"] = "x".repeat(16 * 1024 + 1).into(),
            5 => {
                value["jobs"][0]["request"]["compression"] =
                    serde_json::json!({"numerator": 2, "denominator": 2})
            }
            _ => value["jobs"] = vec![original["jobs"][0].clone(); MAX_RETAINED_JOBS + 1].into(),
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Manager::load(Some(directory.path()), &BTreeMap::new()).is_err());
    }
    fs::write(&path, vec![b' '; MAX_JOURNAL_BYTES as usize + 1]).unwrap();
    assert!(
        Manager::load(Some(directory.path()), &BTreeMap::new())
            .err()
            .unwrap()
            .contains("byte bound")
    );
}

#[tokio::test]
async fn failed_enqueue_and_cancel_checkpoint_leave_live_state_unchanged() {
    let (mut state, id, _) = crate::map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    fs::write(&blocked, b"not a directory").unwrap();
    state.map_package_directory = Some(blocked);
    let next_id = state.map_jobs.lock().await.next_id;
    let input = CreationRequest {
        request: MapRequest::default(),
        preparation: PreparationPreference::Automatic,
    };
    assert!(crate::map_jobs::start(&state, input, None).await.is_err());
    assert_eq!(state.map_jobs.lock().await.next_id, next_id);
    assert!(crate::map_jobs::cancel(&state, id).await.is_err());
    let manager = state.map_jobs.lock().await;
    assert_eq!(manager.jobs[&id].job.state, JobState::Running);
    assert!(
        !manager.jobs[&id]
            .cancelled
            .load(std::sync::atomic::Ordering::Acquire)
    );
}

#[tokio::test]
async fn failed_completion_checkpoint_does_not_publish_or_start_queued_work() {
    let (mut state, id, package) = crate::map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    fs::write(&blocked, b"not a directory").unwrap();
    state.map_package_directory = Some(blocked);
    let request = MapRequest::default();
    let (queued, _) = state
        .map_jobs
        .lock()
        .await
        .enqueue(
            request,
            request.estimate().unwrap(),
            PreparationPlan::fallback(),
        )
        .unwrap();
    crate::map_jobs::finish(&state, id, Ok(package)).await;
    assert!(state.map_packages.read().await.is_empty());
    let manager = state.map_jobs.lock().await;
    assert_eq!(manager.jobs[&id].job.state, JobState::Failed);
    assert!(
        manager.jobs[&id]
            .job
            .error
            .as_ref()
            .unwrap()
            .contains("storage failed")
    );
    assert_eq!(manager.jobs[&queued.id].job.state, JobState::Failed);
    assert_eq!(manager.jobs[&queued.id].job.request, request);
    assert!(!manager.active());
}

#[tokio::test]
async fn recreated_server_loads_completed_history_after_packages() {
    let (mut state, id, package) = crate::map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    state.map_package_directory = Some(directory.path().to_owned());
    crate::map_store::persist(Some(directory.path()), &package).unwrap();
    crate::map_jobs::finish(&state, id, Ok(package)).await;
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.path().to_owned()),
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let restored = crate::AppState::new(&config, "restart").unwrap();
    let history = crate::map_jobs::list(&restored).await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, id);
    assert_eq!(history[0].state, JobState::Completed);
    assert!(
        restored
            .map_packages
            .read()
            .await
            .contains_key(history[0].content_hash.as_ref().unwrap())
    );
}

#[cfg(unix)]
#[tokio::test]
async fn restart_discovers_complete_orphans_without_claiming_job_success() {
    use std::os::unix::fs::PermissionsExt;
    let (mut state, id, package) = crate::map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    state.map_package_directory = Some(directory.path().to_owned());
    persist(Some(directory.path()), &*state.map_jobs.lock().await)
        .await
        .unwrap();
    crate::map_store::persist(Some(directory.path()), &package).unwrap();
    let hash = package.content_hash_hex();
    let jobs = directory.path().join("jobs");
    fs::set_permissions(&jobs, fs::Permissions::from_mode(0o500)).unwrap();
    crate::map_jobs::finish(&state, id, Ok(package.clone())).await;
    fs::set_permissions(&jobs, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(state.map_packages.read().await.is_empty());
    assert_eq!(
        state.map_jobs.lock().await.jobs[&id].job.state,
        JobState::Failed
    );
    let packages = crate::map_store::load(Some(directory.path())).unwrap();
    assert!(packages.contains_key(&hash));
    let restored = Manager::load(Some(directory.path()), &packages).unwrap();
    assert_eq!(restored.jobs[&id].job.state, JobState::Failed);
    assert_eq!(restored.jobs[&id].job.content_hash, None);
}
