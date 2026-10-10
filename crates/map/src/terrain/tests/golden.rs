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
        let destination = terrain
            .forest_opening_center_at(TileCoord::new(origin.x + dx * 192, origin.y + dy * 192))
            .expect("seeded opening node");
        let outcome = crate::find_path(&terrain, origin, destination, 8_192);
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

const FOREST_ROUTES: (usize, &str) = (
    4,
    "1b05ba270a9ff1317cddfc754e03a9082a633a084a95192b6621eaba5e07d7d0",
);

const FALLBACK_DIGESTS: [&str; 3] = [
    "f5e146356df5c83faaf912d9eeccbef022d691f554b4e966a25963ccff8e86ae",
    "0f1cbb2d43b0339368ba5f871268d2dcf39a797d44fa648bb56c49ae02cbd865",
    "0e7ecae1c83b6e3fab3abc7d824cdf3f6478472f68e0efc8c34d7eaa9c857437",
];
const DEFAULT_PACKAGE_DIGEST: &str =
    "247a1e0fa48aa4f5bd6ccea78f9f6fb49eb5e243f2ba01736fd5c625f9b00733";
