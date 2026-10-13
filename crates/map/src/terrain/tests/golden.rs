//! Regression guard on the single current generation, not a compatibility
//! fixture: any deliberate generation change regenerates these digests.
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
fn fallback_chunk_content_is_pinned() {
    let actual = [1, 17, 991]
        .map(|seed| {
            digest(
                &MapChunkGenerator::new([23; 32], seed, 512),
                &[(0, 0), (5, 6), (7, 7), (15, 15)],
            )
        })
        .to_vec();
    assert_eq!(actual, FALLBACK_DIGESTS);
}

#[test]
fn default_package_chunk_content_is_pinned() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let actual = digest(&package.generator(), &[(0, 0), (7, 7), (15, 15)]);
    assert_eq!(actual, DEFAULT_PACKAGE_DIGEST);
}

#[test]
fn flat_forest_routes_from_the_start_glade_are_pinned() {
    let terrain = forest_generator();
    let origin = TileCoord::new(255, 255);
    let mut digest = blake3::Hasher::new();
    let mut paths = 0;
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
        // Whole-map budget: the pin is that forests never wall a node off,
        // not how cheaply a detour around one is found.
        let outcome = crate::find_path(&terrain, origin, destination, 262_144);
        paths += usize::from(matches!(outcome, crate::MovementOutcome::Path(_)));
        digest.update(format!("{destination:?}{outcome:?}").as_bytes());
    }
    assert_eq!(
        (paths, digest.finalize().to_hex().to_string().as_str()),
        FOREST_ROUTES
    );
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

// All eight: before the octile estimate counted the straight remainder of a
// route longer in y than in x, four of these exhausted the search budget.
const FOREST_ROUTES: (usize, &str) = (
    8,
    "47e23b8803057e8fe8aaaa69589b63aee292ab0f4d137c6f9a44ed8c10b91cf3",
);

// Current generation bytes pin the only live wire format. Drawn-sheet variety
// changes these digests without changing the flat forest route pin.
const FALLBACK_DIGESTS: [&str; 3] = [
    "73ba035cc8af889aecd2ab9d6d4fc66e987a7eb42f542c2a7148060c909b7e33",
    "6c22262e64dbda15255c74954e1ccbb585c5890877d076d63bb09f14f707025b",
    "06afe8a6c6178c5d234242f31696c32be7f965af4d01ccdebedf5250e58ef3f4",
];
// Current latitude/elevation ground cover changes the default package.
const DEFAULT_PACKAGE_DIGEST: &str =
    "53da97aff165a1a81a7de5404b28adfecd3b02837ff87f5d3fb784f3edf6cba0";

/// Regression pin of the current generation on scene content rather than
/// transport bytes, so a wire-format change alone cannot move it. Regenerate
/// it with any deliberate generation change, like the pins above.
#[test]
fn default_package_generated_scene_content_is_pinned() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let generator = package.generator();
    let mut digest = blake3::Hasher::new();
    for (x, y) in [(0, 0), (7, 7), (15, 15)] {
        let scene = generator
            .landscape_chunk_with_cancel(x, y, &|| false)
            .expect("chunk");
        digest.update(format!("{scene:?}").as_bytes());
    }
    assert_eq!(
        digest.finalize().to_hex().as_str(),
        "16553e67cfab698876c4f24fe886953ea47a04ec1ecd3ffc32482b3e3df49f7d"
    );
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
