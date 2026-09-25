use super::*;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapRequest,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, ordered_page_root,
};
use std::{fs, sync::Arc};

#[derive(Debug)]
struct Flat;

impl EnvironmentPageProvider for Flat {
    fn page(
        &self,
        key: aoe_map::EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<aoe_map::EnvironmentPage>, aoe_map::EnvironmentPageError> {
        if cancelled() {
            return Err(aoe_map::EnvironmentPageError::Cancelled);
        }
        Ok(Arc::new(aoe_map::EnvironmentPage::Elevation(elevation(
            key.level,
        ))))
    }
}

fn elevation(level: u8) -> ElevationPage {
    let side = if level == 0 { 2 } else { 1 };
    ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![0; usize::from(side).pow(2)],
    }
}

fn package() -> MapPackage {
    MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_920_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: (0..2)
                    .map(|level| PyramidLevel {
                        samples_per_axis: if level == 0 { 2 } else { 1 },
                        ordered_page_root: ordered_page_root(&[elevation(level)]).unwrap(),
                    })
                    .collect(),
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
            hydrology_evidence: None,
        },
    )
    .unwrap()
}

fn resource_node(package: &MapPackage) -> ResourceNode {
    let generator = package
        .generator_with_page_provider(Arc::new(Flat))
        .unwrap();
    (0..2)
        .find_map(|y| (0..2).find_map(|x| generator.chunk(x, y).ok()?.resources.into_iter().next()))
        .expect("fixture resource")
}

fn write_elevation_pages(package_directory: &Path, package: &MapPackage) {
    let root = package_directory
        .join("pages")
        .join(package.content_hash_hex())
        .join("elevation");
    fs::create_dir_all(&root).unwrap();
    for level in 0..2 {
        let page = elevation(level);
        fs::write(
            root.join(format!("{level}-0-0.json")),
            serde_json::to_vec(&page).unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn full_resource_lifecycle_is_observable_after_disk_restart() {
    let package_directory = tempfile::tempdir().unwrap();
    let state_directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = resource_node(&package);
    write_elevation_pages(package_directory.path(), &package);
    let service = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(state_directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();

    let run = exercise_resource_lifecycle(
        service,
        &package,
        package_directory.path(),
        state_directory.path(),
        &node,
    )
    .await
    .unwrap();

    assert_eq!(run.evidence.initial_snapshot_count, 1);
    assert_eq!(run.evidence.resource_mutation_count, 1);
    assert_eq!(run.evidence.delta_snapshot_count, 1);
    assert_eq!(run.evidence.resume_reconnect_snapshot_count, 1);
    assert_eq!(run.evidence.restart_count, 1);
    assert_eq!(run.evidence.persisted_snapshot_replay_count, 1);
    assert_eq!(run.evidence.post_restart_client_snapshot_count, 1);
    assert_eq!(run.evidence.page_residency_recreation_count, 1);
    assert_eq!(run.evidence.stored_map_recreation_count, 1);
    assert_eq!(run.evidence.network_connection_count, 3);
    assert!(run.evidence.resume_token_reconnect_verified);
    assert!(run.evidence.changed_world_id_verified);
    assert!(run.evidence.persisted_snapshot_verified);
    assert!(run.evidence.post_restart_client_snapshot_verified);
    assert!(run.evidence.network_resume_snapshot_verified);
    assert!(run.evidence.network_post_restart_snapshot_verified);
    assert_eq!(run.persisted_snapshot.revision, 1);
    assert_eq!(run.persisted_snapshot.changes.len(), 1);
    assert_eq!(run.restart_provider_resident_pages, 1);
}
