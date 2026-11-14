use super::*;
use std::time::Duration;
mod determinism;
mod fixture;
mod mapped;
mod performance;

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
    let replacement = path.with_extension("corrupt");
    fs::write(&replacement, bytes).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert!(read(&path, [42; 32], 2, &|| false).is_err());
    // Replacing the pathname must not invalidate a live mapping of its inode.
    assert_eq!(
        loaded.walk_at_with_cancel(aoe_core::TileCoord::new(32, 32), &|| false),
        generator.walk_at_with_cancel(aoe_core::TileCoord::new(32, 32), &|| false)
    );
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
    assert_eq!(
        read(&files[0], [42; 32], 2, &|| false)
            .unwrap()
            .chunk_count(),
        4
    );
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
fn creator_map_exceeding_the_index_budget_keeps_lazy_navigation() {
    let directory = tempfile::tempdir().unwrap();
    let request = aoe_map::MapRequest {
        requested_side_meters: 480_000,
        compression: aoe_map::Ratio::new(1, 1).unwrap(),
        ..Default::default()
    };
    let package = MapPackage::new(1, request, Vec::new()).unwrap();
    assert_eq!(package.estimate.tiles_per_side, 240_000);
    prepare_package(&package, Some(directory.path()), &|| false, &|_, _| {}).unwrap();
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
fn streamed_navigation_stops_at_its_storage_budget() {
    let generator = MapChunkGenerator::new([7; 32], 7, 33);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bounded.bin");
    let error = stream::build(
        &generator,
        2,
        2,
        &path,
        [8; 32],
        1,
        stream::Hooks {
            cancelled: &|| false,
            progress: &|_, _| {},
        },
    )
    .unwrap_err();
    assert!(error.contains("navigation storage budget"), "{error}");
    assert!(!path.exists());
    assert!(!generator.has_static_navigation());
}

#[test]
fn streamed_navigation_reports_every_chunk_when_it_completes() {
    let generator = MapChunkGenerator::new([7; 32], 7, 33);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("counted.bin");
    let last = std::sync::Mutex::new((0, 0));
    stream::build(
        &generator,
        2,
        2,
        &path,
        [8; 32],
        MAX_FILE_BYTES,
        stream::Hooks {
            cancelled: &|| false,
            progress: &|done, total| *last.lock().unwrap() = (done, total),
        },
    )
    .unwrap();
    assert_eq!(*last.lock().unwrap(), (4, 4));
}

/// Timing bench on a stored package, not a regression test:
/// `AOE_NAV_BENCH_DIR=<package directory> AOE_NAV_BENCH_HASH=<hex> [AOE_NAV_WORKERS=n]
/// cargo test --release -p aoe-server --lib bench_static_navigation -- --ignored --nocapture`.
/// It also compares the built file byte for byte with the stored one.
#[test]
#[ignore = "needs a stored package"]
fn bench_static_navigation_build() {
    let directory = PathBuf::from(std::env::var("AOE_NAV_BENCH_DIR").unwrap());
    let hash = std::env::var("AOE_NAV_BENCH_HASH").unwrap();
    let workers = std::env::var("AOE_NAV_WORKERS").map_or(4, |value| value.parse().unwrap());
    let package = crate::map_store::load(Some(&directory)).unwrap()[&hash].clone();
    let provider = crate::map_store::PageResidency::open(&directory, &package, &|| false).unwrap();
    let generator = package.generator_with_page_provider(provider).unwrap();
    let side = chunk_side(&generator).unwrap();
    let built = directory
        .join("navigation")
        .join(format!("bench-{}.bin", std::process::id()));
    let started = Instant::now();
    stream::build(
        &generator,
        side,
        workers,
        &built,
        package.content_hash,
        MAX_FILE_BYTES,
        stream::Hooks {
            cancelled: &|| false,
            progress: &|_, _| {},
        },
    )
    .unwrap();
    println!(
        "bench workers={workers} elapsed_ms={} {}",
        started.elapsed().as_millis(),
        stream::step_report()
    );
    let stored = navigation_path(&directory, package.content_hash);
    if let Ok(stored) = fs::read(&stored) {
        let rebuilt = fs::read(&built).unwrap();
        // Chunks land in the file in arrival order, so offsets differ between
        // any two builds; compare each chunk's checksum and bytes, not where
        // it sits.
        let chunk = |file: &[u8], at: usize| {
            let record = &file[HEADER_BYTES + at * INDEX_RECORD_BYTES..][..INDEX_RECORD_BYTES];
            let offset = u64::from_le_bytes(record[..8].try_into().unwrap()) as usize;
            let length = u32::from_le_bytes(record[8..12].try_into().unwrap()) as usize;
            (
                record[12..].to_vec(),
                file[offset..offset + length].to_vec(),
            )
        };
        let same = (0..side * side).all(|at| chunk(&stored, at) == chunk(&rebuilt, at));
        println!(
            "bench chunks_identical_to_stored={same} stored_len={} built_len={}",
            stored.len(),
            rebuilt.len()
        );
    }
    let _ = fs::remove_file(&built);
}
