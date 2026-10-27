use super::*;

#[cfg(target_os = "linux")]
#[test]
fn safe_mapping_survives_truncation_of_an_unsealed_source() {
    use std::io::Write;
    let mut source = tempfile::tempfile().unwrap();
    source.write_all(b"immutable navigation").unwrap();
    let mapping = immutable::map(&mut source, 20, &|| false).unwrap();
    source.set_len(0).unwrap();
    assert_eq!(&mapping[..], b"immutable navigation");
}

#[test]
fn streamed_bake_maps_every_chunk_without_retaining_decoded_tables() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture::flat_world(256);
    let Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    prepare(generator, [83; 32], Some(directory.path()), &|| false).unwrap();
    let snapshot = generator.navigation_snapshot().unwrap();
    assert_eq!(generator.static_navigation_usage().1, 64);
    assert!(snapshot.heap_bytes() <= 64 * 16 + 1024);
    assert_eq!(generator.walk_cache_usage(), (0, 0));
    assert_eq!(generator.portal_cache_usage(), (0, 0));
    let lazy = fixture::flat_world(256);
    let Terrain::Map {
        generator: lazy, ..
    } = lazy.terrain()
    else {
        panic!()
    };
    for y in 0..256 {
        for x in 0..256 {
            let tile = aoe_core::TileCoord::new(x, y);
            assert_eq!(
                generator.walk_at_with_cancel(tile, &|| false),
                lazy.walk_at_with_cancel(tile, &|| false)
            );
        }
    }
}

#[test]
fn active_bake_does_not_block_match_start_or_lazy_queries() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture::flat_world(64);
    let hash = [93; 32];
    let path = navigation_path(directory.path(), hash);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();
    let start = Instant::now();
    prepare_world(&world, hash, Some(directory.path()), &|| false).unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "match startup waited for a bake lock"
    );
    let Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    assert!(!generator.has_static_navigation());
    assert!(
        generator
            .walk_at_with_cancel(aoe_core::TileCoord::new(10, 10), &|| false)
            .unwrap()
            .is_some()
    );
    drop(lock);
    let start = Instant::now();
    while !generator.has_static_navigation() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn france_record_count_keeps_only_a_small_heap_index() {
    // Repeated synthetic chunk contents exercise the complete France-sized
    // file/index path cheaply. This is storage evidence, not France terrain.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("france-synthetic.bin");
    let generator = MapChunkGenerator::new([7; 32], 7, 32);
    let mut bytes = generator
        .prepare_static_navigation_chunk(0, 0, &|| false)
        .unwrap()
        .encode()
        .unwrap();
    let mut writer = stream::Writer::new(&path, [94; 32], 500, MAX_FILE_BYTES).unwrap();
    for at in 0..250_000 {
        bytes[..4].copy_from_slice(&((at % 500) as i32).to_le_bytes());
        bytes[4..8].copy_from_slice(&((at / 500) as i32).to_le_bytes());
        writer.push(at, &bytes).unwrap();
    }
    writer.finish().unwrap();
    let snapshot = read(&path, [94; 32], 500, &|| false).unwrap();
    assert_eq!(snapshot.chunk_count(), 250_000);
    assert!(snapshot.heap_bytes() <= 4_001_024);
    assert!(snapshot.retained_bytes() <= MAX_FILE_BYTES as usize + snapshot.heap_bytes());
    let report = serde_json::json!({"chunks": 250_000, "payload_bytes_per_chunk": bytes.len(),
        "walk_runs_per_chunk": u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
        "resource_nodes_per_chunk": u16::from_le_bytes(bytes[10..12].try_into().unwrap()),
        "portals_per_chunk": u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
        "file_bytes": fs::metadata(path).unwrap().len(), "heap_bytes": snapshot.heap_bytes(), "logical_retained_bytes": snapshot.retained_bytes(),
        "process_status": fs::read_to_string("/proc/self/status").unwrap_or_default().lines().filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:")).collect::<Vec<_>>()});
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/navigation");
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("storage.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("France synthetic records: {report}");
}

#[test]
fn cancellation_during_streaming_removes_staging_without_publication() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture::flat_world(16_000);
    let Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    let calls = std::cell::Cell::new(0);
    let result = prepare(generator, [95; 32], Some(directory.path()), &|| {
        calls.set(calls.get() + 1);
        calls.get() > 2
    });
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(!generator.has_static_navigation());
    assert!(
        fs::read_dir(directory.path().join("navigation"))
            .unwrap()
            .all(|entry| entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "lock"))
    );
}

#[test]
fn concurrent_background_worlds_share_the_result_after_one_world_retires() {
    let directory = tempfile::tempdir().unwrap();
    let first = fixture::flat_world(64);
    let second = fixture::flat_world(64);
    let third = fixture::flat_world(64);
    let hash = [96; 32];
    let path = navigation_path(directory.path(), hash);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();
    for world in [&first, &second, &third] {
        prepare_world(world, hash, Some(directory.path()), &|| false).unwrap();
    }
    drop(first);
    drop(lock);
    let (
        Terrain::Map { generator: two, .. },
        Terrain::Map {
            generator: three, ..
        },
    ) = (second.terrain(), third.terrain())
    else {
        panic!()
    };
    let start = Instant::now();
    while !two.has_static_navigation() || !three.has_static_navigation() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(Arc::ptr_eq(
        &two.navigation_snapshot().unwrap(),
        &three.navigation_snapshot().unwrap()
    ));
}
