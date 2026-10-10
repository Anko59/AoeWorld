use super::*;

#[test]
fn creator_vectors_are_explicit_and_never_selected_by_region() {
    let request = MapRequest {
        requested_side_meters: 1_200_000,
        ..MapRequest::default()
    };
    let mut input: CreationRequest =
        serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
    let absent = PreparationPlan::resolve(input, true).unwrap();
    assert_eq!(absent.hydrology_mode, HydrologyMode::None);
    assert!(
        serde_json::to_value(absent)
            .unwrap()
            .get("hydrology_mode")
            .is_none()
    );
    input.hydrology_mode = HydrologyMode::Vectors;
    assert!(PreparationPlan::resolve(input, true).is_err()); // requires explicit overview
    input.preparation = PreparationPreference::Overview;
    let plan = PreparationPlan::resolve(input, true).unwrap();
    assert_eq!(plan.mode, PreparationMode::Overview);
    assert_eq!(plan.samples_per_axis, 1024);
    assert_eq!(plan.field_axes.unwrap().water, 128);
    assert_eq!(plan.hydrology_mode, HydrologyMode::Vectors);
    assert!(plan.explanation.contains("unobserved class-0 nodata"));
    assert_eq!(
        serde_json::to_value(plan).unwrap()["hydrology_mode"],
        "vectors"
    );
    assert!(PreparationPlan::resolve(input, false).is_err());
    input.request.center_latitude_e7 = -330_000_000;
    assert!(PreparationPlan::resolve(input, true).is_err());
    input.hydrology_mode = HydrologyMode::None;
    assert_eq!(
        PreparationPlan::resolve(input, true)
            .unwrap()
            .hydrology_mode,
        HydrologyMode::None
    );
    let mut value = serde_json::to_value(request).unwrap();
    value["hydrology_mode"] = "invented".into();
    assert!(serde_json::from_value::<CreationRequest>(value).is_err());
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
