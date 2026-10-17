use super::*;
use crate::map_jobs::{OverviewFieldAxes, PreparationMode, launch, tests::state_with_job};
use crate::map_worker::tests::overview_response;

fn overview_plan() -> PreparationPlan {
    PreparationPlan {
        mode: PreparationMode::Overview,
        samples_per_axis: OverviewFieldAxes::LANDSCAPE.elevation,
        field_axes: Some(OverviewFieldAxes::LANDSCAPE),
        hydrology_mode: crate::map_jobs::HydrologyMode::Automatic,
        geographic_millimeters_per_sample: Some(1),
        explanation: "fixture",
    }
}

async fn store(state: &AppState, package: &MapPackage) {
    state
        .map_packages
        .write()
        .await
        .insert(package.content_hash_hex(), package.clone());
}

#[tokio::test]
async fn a_stored_package_is_reused_only_after_presets_turn_reuse_on() {
    let (state, _, _) = state_with_job();
    let package = overview_response(true);
    store(&state, &package).await;
    let request = package.request;
    assert!(reusable(&state, request, &overview_plan()).await.is_none());
    state.reuse_prepared_maps.store(true, Ordering::SeqCst);
    let found = reusable(&state, request, &overview_plan()).await;
    assert_eq!(
        found.map(|p| p.content_hash_hex()),
        Some(package.content_hash_hex())
    );
}

#[tokio::test]
async fn a_stored_package_of_another_request_or_detail_is_not_reused() {
    let (state, _, _) = state_with_job();
    let package = overview_response(true);
    store(&state, &package).await;
    state.reuse_prepared_maps.store(true, Ordering::SeqCst);
    let mut other = package.request;
    other.seed += 1;
    assert!(reusable(&state, other, &overview_plan()).await.is_none());
    let mut coarser = overview_plan();
    coarser.samples_per_axis = 512;
    assert!(reusable(&state, package.request, &coarser).await.is_none());
    let fallback = PreparationPlan::fallback();
    assert!(reusable(&state, package.request, &fallback).await.is_none());
}

#[tokio::test]
async fn a_job_for_a_stored_preset_completes_without_a_worker() {
    let (state, id, _) = state_with_job();
    let package = overview_response(true);
    store(&state, &package).await;
    state.reuse_prepared_maps.store(true, Ordering::SeqCst);
    {
        let mut manager = state.map_jobs.lock().await;
        let entry = manager.jobs.get_mut(&id).expect("job");
        entry.job.request = package.request;
        entry.job.preparation = overview_plan();
    }
    launch(state.clone(), id);
    for _ in 0..100 {
        let job = status(&state, id).await.expect("job");
        if job.state == JobState::Completed {
            assert_eq!(job.content_hash, Some(package.content_hash_hex()));
            return;
        }
        assert_ne!(job.state, JobState::Failed, "{:?}", job.error);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the job was not completed from the stored package");
}

#[test]
fn preset_files_are_bounded_json_arrays_of_requests() {
    let directory = tempfile::tempdir().expect("directory");
    let write = |name: &str, text: &str| {
        let path = directory.path().join(name);
        std::fs::write(&path, text).expect("write");
        path
    };
    let one = serde_json::to_string(&serde_json::json!([{
        "schema_version": 1, "center_latitude_e7": 615_000_000, "center_longitude_e7": 255_000_000,
        "requested_side_meters": 30_000, "compression": {"numerator": 30, "denominator": 1},
        "year_ce": 600, "seed": 1, "reconstruction_profile": "circa600_v1",
        "preparation": "automatic", "hydrology_mode": "automatic"
    }]))
    .expect("json");
    assert_eq!(
        read_presets(&write("one.json", &one)).expect("one").len(),
        1
    );
    assert!(read_presets(&write("bad.json", "{")).is_err());
    let many = format!("[{}]", [&one[1..one.len() - 1]; MAX_PRESETS + 1].join(","));
    assert!(read_presets(&write("many.json", &many)).is_err());
    assert!(read_presets(&directory.path().join("missing.json")).is_err());
}
