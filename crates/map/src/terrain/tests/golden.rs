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
    "112e4fe0358ffbe1f4bd82013d9cfaea9d7f1998a3bd9edb2d617ee9a84af4b0",
);

// Current generation bytes pin the only live wire format. Drawn-sheet variety
// changes these digests without changing the flat forest route pin.
const FALLBACK_DIGESTS: [&str; 3] = [
    "ee7655d59bf5b03996cd87f992591974bd7dd6c366916dd891f3089381f798e2",
    "27e41bad552b4be29aab53d040c94006838f8535ec27452a556ed57f3d8ef578",
    "4cef4ba2534f3241317133baea7a830966588ff0dafbfff92911891895f9a7b6",
];
// Current latitude/elevation ground cover changes the default package.
const DEFAULT_PACKAGE_DIGEST: &str =
    "7fcd500889e7e0b113cf69959c7f7f9c1a33c591f512a3c2331a6b47406e0277";

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
        "1afae3ec104b7728ff69e4ce4b2a2c86a4014922064273c9de466e3f7aa76f01"
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
