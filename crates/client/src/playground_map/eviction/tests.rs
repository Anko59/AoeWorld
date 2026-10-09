use super::*;
use aoe_map::{Chunk, MapChunkGenerator};
use wasm_bindgen_test::wasm_bindgen_test;

fn stable_reference(
    keys: &[(i32, i32)],
    camera: Camera,
    config: WorldConfig,
    preferred: &[(i32, i32)],
) -> Vec<(i32, i32)> {
    let preferred: BTreeSet<_> = preferred.iter().copied().collect();
    let mut coordinates = keys.to_vec();
    // Exact original stable comparator, including total_cmp and final key tie.
    coordinates.sort_by(|left, right| {
        let left_distance = chunk_distance_for(*left, camera, config);
        let right_distance = chunk_distance_for(*right, camera, config);
        preferred
            .contains(left)
            .cmp(&preferred.contains(right))
            .then_with(|| right_distance.total_cmp(&left_distance))
            .then(right.cmp(left))
    });
    coordinates
}

#[wasm_bindgen_test]
fn eviction_retains_exact_stable_reference_suffix_for_512_unique_keys() {
    let config = WorldConfig::new(1024, 512, aoe_core::Seed(1)).unwrap();
    let tile = MapChunkGenerator::new([0; 32], 1, 32)
        .chunk(0, 0)
        .unwrap()
        .tiles[0];
    let source: BTreeMap<_, _> = (0..16)
        .flat_map(|y| {
            (0..32).map(move |x| {
                (
                    (x, y),
                    CachedChunk::Legacy(Chunk {
                        x,
                        y,
                        tiles: vec![tile],
                        resources: Vec::new(),
                    }),
                )
            })
        })
        .collect();
    let keys: Vec<_> = source.keys().copied().collect();
    assert_eq!(keys.len(), 512);
    for center in [
        [512.0, 256.0],
        [16.0, 16.0],
        [0.0, -0.0],
        [-0.0, 0.0],
        [f64::NAN, 256.0],
        [f64::from_bits(0xfff8_0000_0000_0001), 256.0],
    ] {
        let camera = Camera {
            center,
            zoom: 1.0,
            viewport: [800.0, 600.0],
            focus_elevation_meters: 0.0,
        };
        for preferred in [&[][..], &[(0, 0), (31, 15), (15, 7)][..]] {
            let order = stable_reference(&keys, camera, config, preferred);
            for capacity in [0, 1, 64, 511, 512] {
                let mut chunks = source.clone();
                let mut discovered: BTreeSet<_> = keys.iter().copied().collect();
                let (removed, _) = evict_distant_chunks_with_limits(
                    &mut chunks,
                    &mut discovered,
                    camera,
                    config,
                    capacity,
                    usize::MAX,
                    preferred,
                );
                let expected: BTreeSet<_> = order[512 - capacity..].iter().copied().collect();
                assert_eq!(chunks.keys().copied().collect::<BTreeSet<_>>(), expected);
                assert_eq!(discovered, expected);
                assert_eq!(removed, capacity < 512);
            }
        }
    }
}

#[wasm_bindgen_test]
fn one_at_a_time_eviction_matches_old_stable_sequence_including_distance_ties() {
    let config = WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap();
    let mut chunks: BTreeMap<_, _> = (0..8)
        .flat_map(|y| {
            (0..8).map(move |x| {
                (
                    (x, y),
                    CachedChunk::Legacy(Chunk {
                        x,
                        y,
                        tiles: Vec::new(),
                        resources: Vec::new(),
                    }),
                )
            })
        })
        .collect();
    let keys: Vec<_> = chunks.keys().copied().collect();
    let camera = Camera {
        center: [128.0, 128.0],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let preferred = [(0, 0), (7, 7)];
    let order = stable_reference(&keys, camera, config, &preferred);
    let mut discovered: BTreeSet<_> = keys.iter().copied().collect();
    for (index, expected) in order.into_iter().enumerate() {
        let before = discovered.clone();
        evict_distant_chunks_with_limits(
            &mut chunks,
            &mut discovered,
            camera,
            config,
            63 - index,
            usize::MAX,
            &preferred,
        );
        let removed: Vec<_> = before.difference(&discovered).copied().collect();
        assert_eq!(removed, vec![expected]);
    }
}
