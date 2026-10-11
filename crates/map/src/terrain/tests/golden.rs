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
    "1f68bd6d1896966fa8d7df3b24a45209d4b45b621ec4e2ea12c162a33f2d0255",
    "ecf308800b39593c23cc6e86f0c029277fe2e751ae6a983d63746d0326df1799",
    "fc2edb936af1172eac794affc0c9f36617ea14a5c2362d54e5d5bfece950f243",
];
const DEFAULT_PACKAGE_DIGEST: &str =
    "a7c9b467bad6228d96f1da5fdbab0ec3f9fc867022bc51775b4b204018648468";

#[test]
fn default_package_generated_chunk_content_is_pinned() {
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
        "47807de0cb4883cafeb2e5d482ae1faa5ea4614e5499d735f1321142130a750b"
    );
}

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
        "39420d801c3728593fcfb97f6c915a72c1c50e697cccc67ae092efcc9b6a61bd"
    );
}
