use super::*;

#[test]
fn chunk_halo_is_lazy_fixed_and_checks_cancellation_on_hits() {
    let (generator, source) = generator(None);
    let cancelled = Cell::new(false);
    let callback = || cancelled.get();
    let memo = ChunkMemo::new_chunk(&generator, TileCoord::new(32, 32), &callback);
    assert_eq!(source.calls.load(Ordering::Relaxed), 0);
    let first = memo.base(TileCoord::new(32, 32)).unwrap();
    let per_tile = source.calls.load(Ordering::Relaxed);
    assert_eq!(per_tile, 20);
    assert_eq!(memo.base(TileCoord::new(32, 32)).unwrap(), first);
    assert_eq!(source.calls.load(Ordering::Relaxed), per_tile);
    cancelled.set(true);
    assert_eq!(
        memo.base(TileCoord::new(32, 32)),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(
        memo.candidate(TileCoord::new(32, 32)),
        Err(EnvironmentPageError::Cancelled)
    );
    cancelled.set(false);
    for y in 30..66 {
        for x in 30..66 {
            memo.base(TileCoord::new(x, y)).unwrap();
        }
    }
    assert_eq!(
        memo.entries
            .borrow()
            .iter()
            .filter(|entry| entry.base.is_some())
            .count(),
        1296
    );
    assert_eq!(source.calls.load(Ordering::Relaxed), 1296 * per_tile);
    for y in 30..66 {
        for x in 30..66 {
            memo.base(TileCoord::new(x, y)).unwrap();
        }
    }
    assert_eq!(source.calls.load(Ordering::Relaxed), 1296 * per_tile);
}

#[test]
fn production_chunk_matches_standalone_point_bytes_with_bounded_source_work() {
    let (generator, source) = generator(None);
    let chunk = generator
        .landscape_chunk_with_cancel(1, 1, &|| false)
        .unwrap();
    let chunk_calls = source.calls.load(Ordering::Relaxed);
    assert!(chunk_calls <= 36 * 36 * 20, "{chunk_calls}");
    let mut reference = LandscapeChunk {
        x: 1,
        y: 1,
        tiles: Vec::new(),
        resources: Vec::new(),
        decorations: Vec::new(),
    };
    for y in 32..64 {
        for x in 32..64 {
            let point = generator
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .unwrap()
                .unwrap();
            reference.tiles.push(point.tile);
            if let Some(resource) = point.resource {
                reference.resources.push(resource);
            }
            if let Some(decoration) = point.decoration {
                reference.decorations.push(decoration);
            }
        }
    }
    let standalone_calls = source.calls.load(Ordering::Relaxed) - chunk_calls;
    // Resource patches read base tiles lazily, so standalone points already
    // share little; the chunk halo still reads each source tile at most once.
    assert!(
        standalone_calls > chunk_calls,
        "point={standalone_calls} chunk={chunk_calls}"
    );
    assert_eq!(
        crate::CompactChunk::encode(&chunk).unwrap(),
        crate::CompactChunk::encode(&reference).unwrap()
    );
}

#[test]
fn chunk_memo_does_not_retain_source_errors_or_fetch_outside_world() {
    for error in [EnvironmentPageError::Missing, EnvironmentPageError::Corrupt] {
        let (generator, source) = generator(Some(error));
        let memo = ChunkMemo::new_chunk(&generator, TileCoord::new(0, 0), &|| false);
        assert_eq!(memo.base(TileCoord::new(0, 0)), Err(error));
        let calls = source.calls.load(Ordering::Relaxed);
        assert_eq!(memo.base(TileCoord::new(0, 0)), Err(error));
        assert!(source.calls.load(Ordering::Relaxed) > calls);
        let calls = source.calls.load(Ordering::Relaxed);
        assert_eq!(memo.base(TileCoord::new(-1, 0)), Ok(None));
        assert_eq!(source.calls.load(Ordering::Relaxed), calls);
        assert_eq!(
            generator.landscape_chunk_with_cancel(0, 0, &|| false),
            Err(error)
        );
        assert_eq!(
            generator.landscape_chunk_with_cancel(0, 0, &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
    }
}

#[test]
fn provider_chunk_cliff_evaluations_are_bounded_by_distinct_cells() {
    let (mut generator, _) = generator(None);
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        side: 2,
        error: None,
    });
    generator.provider = Some(source.clone());
    Arc::make_mut(generator.provider_environment.as_mut().unwrap()).samples_per_axis = 2;
    let memo = ChunkMemo::new_chunk(&generator, TileCoord::new(34, 34), &|| false);
    let mut cells = std::collections::BTreeSet::new();
    for y in 32..68 {
        for x in 32..68 {
            let position = TileCoord::new(x, y);
            assert!(memo.base(position).unwrap().is_some());
            cells.insert((x / 3, y / 3));
        }
    }
    // Each tile interpolates its height and four corners (20 page calls).
    // Each cliff decision interpolates four cell corners (16 page calls).
    // Flat dry input ensures there are no additional physical-water reads.
    let calls = source.calls.load(Ordering::Relaxed);
    let evaluations = (calls - 1296 * 20) / 16;
    assert_eq!(cells.len(), 169);
    assert!(evaluations > 0);
    assert!(
        evaluations <= cells.len(),
        "{evaluations} evaluations for {} cells",
        cells.len()
    );
    for y in 32..68 {
        for x in 32..68 {
            let position = TileCoord::new(x, y);
            let expected = generator.landscape_base_at(position, &|| false).unwrap();
            assert_eq!(memo.base(position).unwrap(), expected);
        }
    }
}
