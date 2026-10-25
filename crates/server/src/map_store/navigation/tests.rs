use super::*;

#[test]
fn persisted_navigation_loads_without_terrain_composition_and_rejects_corruption() {
    let directory = tempfile::tempdir().unwrap();
    let generator = MapChunkGenerator::new([7; 32], 7, 33);
    prepare(&generator, [42; 32], Some(directory.path()), &|| false).unwrap();
    let loaded = MapChunkGenerator::new([7; 32], 7, 33);
    prepare(&loaded, [42; 32], Some(directory.path()), &|| false).unwrap();
    assert!(loaded.has_static_navigation());
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
    assert_eq!(
        loaded.walk_at_with_cancel(aoe_core::TileCoord::new(32, 32), &|| false),
        generator.walk_at_with_cancel(aoe_core::TileCoord::new(32, 32), &|| false)
    );
    let path = fs::read_dir(directory.path().join("navigation"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .unwrap();
    assert!(read(&path, [0; 32], 2, &|| false).is_err());
    assert!(read(&path, [42; 32], 2, &|| true).is_err());
    let mut bytes = fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(read(&path, [42; 32], 2, &|| false).is_err());
}

#[test]
fn cancelled_preparation_never_publishes_a_partial_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let generator = MapChunkGenerator::new([7; 32], 7, 64);
    assert!(prepare(&generator, [42; 32], Some(directory.path()), &|| true).is_err());
    assert!(!generator.has_static_navigation());
    assert!(!directory.path().join("navigation").exists());
}

#[test]
fn concurrent_package_preparation_publishes_one_complete_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let root = directory.path();
            tasks.push(scope.spawn(move || {
                let generator = MapChunkGenerator::new([7; 32], 7, 33);
                prepare(&generator, [42; 32], Some(root), &|| false).unwrap();
                assert!(generator.has_static_navigation());
            }));
        }
        for task in tasks {
            task.join().unwrap();
        }
    });
    let files: Vec<_> = fs::read_dir(directory.path().join("navigation"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(read(&files[0], [42; 32], 2, &|| false).unwrap().len(), 4);
}

#[test]
fn simultaneous_worlds_share_one_verified_package_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let first = MapChunkGenerator::new([71; 32], 7, 33);
    let second = MapChunkGenerator::new([71; 32], 7, 33);
    prepare(&first, [71; 32], Some(directory.path()), &|| false).unwrap();
    prepare(&second, [71; 32], Some(directory.path()), &|| false).unwrap();
    let one = first.navigation_snapshot().unwrap();
    let two = second.navigation_snapshot().unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&one, &two),
        "match replacement must not decode a second map"
    );
    assert_eq!(
        second.walk_cache_usage(),
        (0, 0),
        "mutable caches remain world-local"
    );
}

#[test]
fn creator_large_uncompressed_map_uses_lazy_navigation_without_baking() {
    let directory = tempfile::tempdir().unwrap();
    let request = aoe_map::MapRequest {
        requested_side_meters: 480_000,
        compression: aoe_map::Ratio::new(1, 1).unwrap(),
        ..Default::default()
    };
    let package = MapPackage::new(1, request, Vec::new()).unwrap();
    assert_eq!(package.estimate.tiles_per_side, 240_000);
    prepare_package(&package, Some(directory.path()), &|| false).unwrap();
    let generator = package.generator();
    prepare(
        &generator,
        package.content_hash,
        Some(directory.path()),
        &|| false,
    )
    .unwrap();
    assert!(!generator.has_static_navigation());
    assert!(!directory.path().join("navigation").exists());
    generator
        .prepare_navigation(&aoe_map::ResourceOverlay::default(), &|| false)
        .unwrap();
    assert_eq!(generator.prepared_navigation_entries(), 0);
    assert_eq!(generator.walk_cache_usage(), (0, 0));
}

#[test]
fn eager_navigation_stops_at_its_retained_memory_budget() {
    let generator = MapChunkGenerator::new([7; 32], 7, 33);
    let error = build(&generator, 2, 2, 1, &|| false).unwrap_err();
    assert!(error.contains("navigation memory budget"), "{error}");
    assert!(!generator.has_static_navigation());
}
