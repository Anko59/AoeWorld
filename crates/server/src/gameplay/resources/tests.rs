use super::*;
use aoe_map::ResourceOverlay;

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
fn elevation(level: u8) -> aoe_map::ElevationPage {
    let side = if level == 0 { 2 } else { 1 };
    aoe_map::ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![0; usize::from(side).pow(2)],
    }
}
fn package() -> MapPackage {
    use aoe_map::*;
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
        },
    )
    .unwrap()
}
fn node(package: &MapPackage) -> aoe_map::ResourceNode {
    let generator = package
        .generator_with_page_provider(Arc::new(Flat))
        .unwrap();
    (0..2)
        .find_map(|y| (0..2).find_map(|x| generator.chunk(x, y).ok()?.resources.into_iter().next()))
        .expect("fixture resource")
}

#[tokio::test]
async fn persisted_depletion_survives_service_recreation_and_failed_save_is_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let service = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let result = service
        .deplete_resource_persisted(node.id, node.initial_amount)
        .await
        .unwrap();
    assert!(result.directory_synced);
    assert!(result.depletion.became_nonblocking);
    let snapshot = service.resource_snapshot().await.unwrap();
    let restored = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(restored.resource_snapshot().await.unwrap(), snapshot);
    let failed_directory = directory.path().join("not-a-directory");
    std::fs::write(&failed_directory, b"sentinel").unwrap();
    let mut failed =
        GameplayService::from_stored_map(package, Some(Arc::new(Flat)), None, &|| false)
            .unwrap()
            .unwrap();
    failed.resource_directory = Some(failed_directory);
    let before = failed.resource_snapshot().await.unwrap();
    assert!(failed.deplete_resource_persisted(node.id, 1).await.is_err());
    assert_eq!(failed.resource_snapshot().await.unwrap(), before);
}

#[tokio::test]
async fn concurrent_changes_serialize_and_corrupt_restore_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let service = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let (first, second) = tokio::join!(
        service.deplete_resource_persisted(node.id, 1),
        service.deplete_resource_persisted(node.id, 1)
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(service.resource_snapshot().await.unwrap().revision, 2);
    let overlays = directory.path().join("resource-overlays");
    let saved = std::fs::read_dir(&overlays)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(saved, b"{broken").unwrap();
    assert!(
        GameplayService::from_stored_map(
            package,
            Some(Arc::new(Flat)),
            Some(directory.path().to_owned()),
            &|| { false }
        )
        .is_err()
    );
}

#[test]
fn bounded_storage_rejects_oversized_json_and_wrong_map_on_restore() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let overlays = directory.path().join("resource-overlays");
    let snapshot = ResourceOverlay::default().snapshot(package.content_hash);
    store::save(&overlays, &snapshot, 0).unwrap();
    let saved = std::fs::read_dir(&overlays)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&saved)
        .unwrap();
    file.set_len(4 * 1024 * 1024 + 1).unwrap();
    assert!(matches!(
        store::load(&overlays, package.content_hash),
        Err(ResourceLifecycleError::TooLarge)
    ));
    let mut wrong = snapshot;
    wrong.map_content_hash = [7; 32];
    std::fs::write(saved, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(
        GameplayService::from_stored_map(
            package,
            Some(Arc::new(Flat)),
            Some(directory.path().to_owned()),
            &|| { false }
        )
        .is_err()
    );
}

#[tokio::test]
async fn stale_service_cannot_overwrite_a_newer_persisted_revision() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let first = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let stale = GameplayService::from_stored_map(
        package,
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let before = stale.resource_snapshot().await.unwrap();
    first.deplete_resource_persisted(node.id, 1).await.unwrap();
    assert!(matches!(
        stale.deplete_resource_persisted(node.id, 2).await,
        Err(ResourceLifecycleError::StaleRevision)
    ));
    assert_eq!(stale.resource_snapshot().await.unwrap(), before);
    assert_eq!(first.resource_snapshot().await.unwrap().revision, 1);
}

#[test]
fn interrupted_temporary_snapshot_is_recovered_without_accumulating_files() {
    let directory = tempfile::tempdir().unwrap();
    let snapshot = ResourceOverlay::default().snapshot([5; 32]);
    let temporary = directory
        .path()
        .join(format!("{}.resource-tmp", "05".repeat(32)));
    std::fs::write(&temporary, b"incomplete prior write").unwrap();
    assert!(store::save(directory.path(), &snapshot, 0).unwrap());
    assert!(!temporary.exists());
    assert_eq!(
        store::load(directory.path(), [5; 32]).unwrap(),
        Some(snapshot)
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
