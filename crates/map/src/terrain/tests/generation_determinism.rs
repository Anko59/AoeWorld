//! Determinism properties of the current generation: independently built
//! generators emit identical chunks, and forests never wall off a route.
use super::*;
use crate::{CompactChunk, MapPackage, MapRequest};

fn digest(generator: &MapChunkGenerator, chunks: &[(i32, i32)]) -> String {
    let mut digest = blake3::Hasher::new();
    for &(x, y) in chunks {
        let scene = generator
            .landscape_chunk_with_cancel(x, y, &|| false)
            .expect("current chunk");
        let encoded = CompactChunk::encode(&scene).expect("current encoding");
        digest.update(encoded.payload_hex.as_bytes());
    }
    digest.finalize().to_hex().to_string()
}

#[test]
fn chunk_content_is_deterministic_and_keyed_by_seed() {
    const CHUNKS: [(i32, i32); 4] = [(0, 0), (5, 6), (7, 7), (15, 15)];
    let run =
        || [1, 17, 991].map(|seed| digest(&MapChunkGenerator::new([23; 32], seed, 512), &CHUNKS));
    let first = run();
    assert_eq!(first, run());
    assert!(first[0] != first[1] && first[1] != first[2] && first[0] != first[2]);
    let package = || MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    assert_eq!(
        digest(&package().generator(), &CHUNKS),
        digest(&package().generator(), &CHUNKS)
    );
}

#[test]
fn flat_forest_routes_from_the_start_glade_all_reach_their_node() {
    let terrain = forest_generator();
    let origin = TileCoord::new(255, 255);
    for (dx, dy) in [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        let node = terrain
            .forest_opening_center_at(TileCoord::new(origin.x + dx * 192, origin.y + dy * 192))
            .expect("seeded trail node");
        // A node is open only inside a large forest: route to the nearest
        // tile of its row with open ground all around it (a lone gap between
        // the trees of a dense stand is not a destination).
        let open = |tile: TileCoord| {
            terrain
                .tile_and_node_with_cancel(tile, &|| false)
                .expect("flat source")
                .is_some_and(|(tile, node)| tile.passable && node.is_none())
        };
        let destination = (0..96)
            .flat_map(|offset| [node.x - offset, node.x + offset])
            .map(|x| TileCoord::new(x.clamp(1, 510), node.y.clamp(1, 510)))
            .find(|tile| {
                (-1..=1).all(|dy| (-1..=1).all(|dx| open(TileCoord::new(tile.x + dx, tile.y + dy))))
            })
            .expect("open ground beside every node");
        // Whole-map budget: forests never wall a node off, however costly the
        // detour. Before the octile estimate counted the straight remainder of
        // a route longer in y than in x, four of these exhausted the budget.
        let outcome = crate::find_path(&terrain, origin, destination, 262_144);
        assert!(
            matches!(outcome, crate::MovementOutcome::Path(_)),
            "{destination:?}: {outcome:?}"
        );
    }
}

/// Flat dry temperate forest (PNV class 9) with one elevation sample.
fn forest_generator() -> MapChunkGenerator {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![9],
    };
    let level = |root| crate::FieldPyramid {
        levels: vec![crate::PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: root,
        }],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: level(crate::ordered_page_root(std::slice::from_ref(&elevation)).unwrap()),
        vegetation: Some(level(
            crate::ordered_biome_page_root(std::slice::from_ref(&biome)).unwrap(),
        )),
        ..PreparedEnvironment::default()
    };
    MapChunkGenerator::new([29; 32], 3, 512)
        .with_prepared_elevation(Ratio::new(1, 1).unwrap(), &environment, vec![elevation])
        .unwrap()
        .with_prepared_biomes(&environment, vec![biome])
        .unwrap()
}

/// Sheet selection and ground variety preserve physical terrain. Retain all
/// generator/chunk cases from the former plumbing-only assertion.
#[test]
fn ground_sheet_plumbing_coexists_with_generated_variety() {
    const DRAWN_MATERIALS: [GroundMaterial; 3] = [
        GroundMaterial::GrassDirt,
        GroundMaterial::WornGrass,
        GroundMaterial::Gravel,
    ];
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let fallbacks = [1, 17, 991].map(|seed| MapChunkGenerator::new([23; 32], seed, 512));
    let mut observed = std::collections::BTreeSet::new();
    for generator in [package.generator()].into_iter().chain(fallbacks) {
        for (x, y) in [(0, 0), (7, 7), (15, 15)] {
            let scene = generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .expect("chunk");
            for tile in scene.tiles {
                if DRAWN_MATERIALS.contains(&tile.terrain.material) {
                    assert!(tile.terrain.passable);
                    assert_eq!(tile.terrain.water, WaterKind::None);
                    observed.insert(tile.terrain.material as u8);
                }
            }
        }
    }
    assert!(observed.contains(&(GroundMaterial::GrassDirt as u8)));
    assert!(observed.contains(&(GroundMaterial::WornGrass as u8)));
}

fn streamed(key: &[u8; 32], domain: &[u8], x: i32, y: i32, suffix: &[u8]) -> u64 {
    let mut hash = blake3::Hasher::new_keyed(key);
    hash.update(domain);
    hash.update(&x.to_le_bytes());
    hash.update(&y.to_le_bytes());
    hash.update(suffix);
    u64::from_le_bytes(hash.finalize().as_bytes()[..8].try_into().unwrap())
}

#[test]
fn coordinate_hashes_keep_the_exact_keyed_byte_stream() {
    let generator = generator(7);
    for domain in [
        b"".as_slice(),
        b"gold-patch-v2",
        b"forest-corner",
        &[7; 119],
    ] {
        for (x, y) in [(0, 0), (-33, 17), (i32::MIN, i32::MAX)] {
            assert_eq!(
                unsigned_noise(generator.geography_key, domain, x, y),
                streamed(&generator.geography_key, domain, x, y, &[])
            );
            for vertex in [0, 7, 15, 255] {
                assert_eq!(
                    geometry::cell_value(&generator, domain, (x, y), vertex),
                    streamed(&generator.geography_key, domain, x, y, &[vertex])
                );
            }
        }
    }
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(b"resource-detail");
    hash.update(&generator.procedural_seed.to_le_bytes());
    assert_eq!(
        resources::detail_key(&generator),
        *hash.finalize().as_bytes()
    );
}
