use super::*;

#[test]
fn creator_requests_hydrology_by_default_for_overview_and_finland() {
    for (latitude, longitude, side) in [
        (480_000_000, 20_000_000, 1_200_000),
        (615_000_000, 255_000_000, 30_000),
    ] {
        let request = MapRequest {
            center_latitude_e7: latitude,
            center_longitude_e7: longitude,
            requested_side_meters: side,
            ..MapRequest::default()
        };
        let mut value = serde_json::to_value(request).unwrap();
        value["preparation"] = "overview".into();
        let input: CreationRequest = serde_json::from_value(value).unwrap();
        let plan = PreparationPlan::resolve(input, true).unwrap();
        assert_eq!(plan.mode, PreparationMode::Overview);
        assert_eq!(plan.hydrology_mode, HydrologyMode::Automatic);
        assert_eq!(plan.samples_per_axis, 1024);
        assert_eq!(plan.field_axes.unwrap().water, 1024);
    }
}

#[tokio::test]
async fn durable_submission_key_cannot_switch_hydrology_mode() {
    use crate::map_jobs::submission::{Identity, existing};
    let (state, id, _) = crate::map_jobs::tests::state_with_job();
    let mut manager = state.map_jobs.lock().await;
    let identity = Identity::new(Some("a".repeat(32)), PreparationPreference::Overview)
        .unwrap()
        .unwrap();
    let entry = manager.jobs.get_mut(&id).unwrap();
    let request = entry.job.request;
    entry.submission = Some(identity.clone());
    assert!(
        existing(&manager, Some(&identity), request)
            .unwrap()
            .is_some()
    );
    let mut changed = identity;
    changed.hydrology_mode = HydrologyMode::Vectors;
    assert!(matches!(
        existing(&manager, Some(&changed), request),
        Err(crate::map_jobs::StartError::Conflict(_))
    ));
    let request = MapRequest {
        requested_side_meters: 1_200_000,
        ..request
    };
    let entry = manager.jobs.get_mut(&id).unwrap();
    entry.job.request = request;
    entry.job.estimate = request.estimate().unwrap();
    entry.job.preparation = PreparationPlan::resolve(
        CreationRequest {
            request,
            preparation: PreparationPreference::Overview,
            hydrology_mode: HydrologyMode::Vectors,
        },
        true,
    )
    .unwrap();
    entry.job.state = crate::map_jobs::JobState::Failed;
    entry.submission = Some(changed);
    let directory = tempfile::tempdir().unwrap();
    crate::map_jobs::journal::persist(Some(directory.path()), &manager)
        .await
        .unwrap();
    let restored =
        crate::map_jobs::Manager::load(Some(directory.path()), &std::collections::BTreeMap::new())
            .unwrap();
    assert_eq!(
        restored.jobs[&id].job.preparation.hydrology_mode,
        HydrologyMode::Vectors
    );
}

#[tokio::test]
async fn automatic_creator_accepts_covered_and_uncovered_requests_and_publishes_the_note() {
    for (latitude, longitude, outside) in [
        (480_000_000, 20_000_000, false),
        (-178_000_000, 1_798_000_000, true),
    ] {
        let request = MapRequest {
            center_latitude_e7: latitude,
            center_longitude_e7: longitude,
            ..MapRequest::default()
        };
        let input: CreationRequest =
            serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
        let plan = PreparationPlan::resolve(input, true).unwrap();
        assert_eq!(plan.mode, PreparationMode::Overview);
        assert_eq!(plan.hydrology_mode, HydrologyMode::Automatic);
        let (state, _, _) = crate::map_jobs::tests::state_with_job();
        let (job, _) = state
            .map_jobs
            .lock()
            .await
            .enqueue(request, request.estimate().unwrap(), plan)
            .unwrap();
        let package = aoe_map::MapPackage::new(1, request, Vec::new()).unwrap();
        let package = if outside {
            package
                .with_preparation_note(aoe_map::PreparationNote::OutsideRiverCoverage)
                .unwrap()
        } else {
            package
        };
        let hash = package.content_hash_hex();
        crate::map_jobs::finish(&state, job.id, Ok(package)).await;
        let completed = crate::map_jobs::status(&state, job.id).await.unwrap();
        assert_eq!(completed.preparation_notes.len(), usize::from(outside));
        assert_eq!(completed.content_hash.as_deref(), Some(hash.as_str()));
        assert_eq!(
            state.map_packages.read().await[&hash].preparation_notes,
            completed.preparation_notes
        );
    }
}
