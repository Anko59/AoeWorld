use super::*;
use crate::map_jobs::{self, CreationRequest, Manager, journal};
use std::collections::BTreeMap;

#[tokio::test]
async fn concurrent_retries_share_one_durable_job_and_survive_restart() {
    let (mut state, _, _) = map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    state.map_package_directory = Some(directory.path().to_owned());
    let input = CreationRequest {
        request: MapRequest::default(),
        preparation: PreparationPreference::Automatic,
    };
    let key = "1234567890abcdef1234567890abcdef".to_owned();
    let mut equivalent = input;
    equivalent.request.compression.numerator *= 2;
    equivalent.request.compression.denominator *= 2;
    let (first, second) = tokio::join!(
        map_jobs::start(&state, input, Some(key.clone())),
        map_jobs::start(&state, equivalent, Some(key.clone()))
    );
    assert_eq!(first.unwrap().id, second.unwrap().id);
    assert_eq!(state.map_jobs.lock().await.jobs.len(), 2);
    let restored = Manager::load(Some(directory.path()), &BTreeMap::new()).unwrap();
    let expected = existing(
        &restored,
        Identity::new(Some(key.clone()), input.preparation)
            .unwrap()
            .as_ref(),
        input.request,
    )
    .unwrap()
    .unwrap();
    assert_eq!(expected.state, map_jobs::JobState::Failed);
    *state.map_jobs.lock().await = restored;
    let retried = map_jobs::start(&state, input, Some(key)).await.unwrap();
    assert_eq!(retried.id, expected.id);
    assert_eq!(retried.state, expected.state);
    assert_eq!(state.map_jobs.lock().await.jobs.len(), 2);
}

#[tokio::test]
async fn reused_key_cannot_change_request_or_preference_and_invalid_keys_are_rejected() {
    let (state, _, _) = map_jobs::tests::state_with_job();
    let mut input = CreationRequest {
        request: MapRequest::default(),
        preparation: PreparationPreference::Automatic,
    };
    let key = "a".repeat(32);
    map_jobs::start(&state, input, Some(key.clone()))
        .await
        .unwrap();
    input.request.seed += 1;
    assert!(matches!(
        map_jobs::start(&state, input, Some(key.clone())).await,
        Err(StartError::Conflict(_))
    ));
    input.request.seed -= 1;
    input.preparation = PreparationPreference::Overview;
    assert!(matches!(
        map_jobs::start(&state, input, Some(key)).await,
        Err(StartError::Conflict(_))
    ));
    for invalid in ["".to_owned(), "a".repeat(33), "G".repeat(32)] {
        assert!(matches!(
            map_jobs::start(&state, input, Some(invalid)).await,
            Err(StartError::Invalid(_))
        ));
    }
    assert_eq!(state.map_jobs.lock().await.jobs.len(), 2);
}

#[tokio::test]
async fn legacy_journal_loads_and_duplicate_or_invalid_submission_keys_fail() {
    let (state, _, _) = map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    journal::persist(Some(directory.path()), &*state.map_jobs.lock().await)
        .await
        .unwrap();
    let path = directory.path().join("jobs/history.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["schema"] = 1.into();
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Manager::load(Some(directory.path()), &BTreeMap::new()).is_ok());
    value["schema"] = 2.into();
    value["jobs"][0]["submission"] = serde_json::json!({"key": "bad", "preference": "automatic"});
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Manager::load(Some(directory.path()), &BTreeMap::new()).is_err());
    value["jobs"][0]["submission"]["key"] = "a".repeat(32).into();
    let mut duplicate = value["jobs"][0].clone();
    duplicate["id"] = 1.into();
    value["jobs"].as_array_mut().unwrap().push(duplicate);
    value["next_id"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Manager::load(Some(directory.path()), &BTreeMap::new()).is_err());
}

#[tokio::test]
async fn failed_checkpoint_does_not_reserve_a_key_or_accept_a_job() {
    let (mut state, _, _) = map_jobs::tests::state_with_job();
    let directory = tempfile::tempdir().unwrap();
    state.map_package_directory = Some(directory.path().to_owned());
    std::fs::write(directory.path().join("jobs"), b"obstruct directory").unwrap();
    let input = CreationRequest {
        request: MapRequest::default(),
        preparation: PreparationPreference::Automatic,
    };
    let key = "b".repeat(32);
    assert!(matches!(
        map_jobs::start(&state, input, Some(key.clone())).await,
        Err(StartError::Storage(_))
    ));
    assert_eq!(state.map_jobs.lock().await.jobs.len(), 1);
    std::fs::remove_file(directory.path().join("jobs")).unwrap();
    let job = map_jobs::start(&state, input, Some(key.clone()))
        .await
        .unwrap();
    assert_eq!(job.id, 1);
    assert_eq!(
        map_jobs::start(&state, input, Some(key)).await.unwrap().id,
        1
    );
    assert_eq!(state.map_jobs.lock().await.jobs.len(), 2);
}
