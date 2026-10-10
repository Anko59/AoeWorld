//! Trees take source-backed visual families from the raw PNV class.
use super::*;

const CLASSES: [Option<u8>; 10] = [
    Some(1),
    Some(4),
    Some(8),
    Some(9),
    Some(13),
    Some(14),
    Some(15),
    Some(17),
    Some(200),
    None,
];

fn with_class(mut generator: MapChunkGenerator, class: Option<u8>) -> MapChunkGenerator {
    generator.biome = class.map(|class| {
        Arc::new(PreparedBiome::new(
            1,
            [(
                (0, 0),
                PotentialBiomePage {
                    level: 0,
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    potential_biome_class: vec![class],
                },
            )]
            .into(),
        ))
    });
    generator
}

fn classed(seed: u64, width: i32, class: Option<u8>) -> MapChunkGenerator {
    with_class(flat(seed, width), class)
}

fn scene(generator: &MapChunkGenerator, x: i32, y: i32) -> LandscapeChunk {
    generator
        .landscape_chunk_with_cancel(x, y, &|| false)
        .unwrap()
}

#[test]
fn tree_families_follow_source_classes_and_other_resources_stay_generic() {
    let mut trees = 0;
    let mut conifers = 0;
    let mut mixed = std::collections::BTreeSet::new();
    for seed in [0, 1, 7, 255] {
        for class in CLASSES {
            for (width, x, y) in [(256, 0, 0), (256, 3, 3), (50, 1, 1)] {
                let generator = classed(seed, width, class);
                let chunk = scene(&generator, x, y);
                let encoded = CompactChunk::encode(&chunk).unwrap();
                assert!(encoded.payload_hex.starts_with("04"));
                assert_eq!(encoded.decode().unwrap(), chunk);
                for resource in &chunk.resources {
                    if resource.node.object != ObjectKind::Tree {
                        assert_eq!(resource.visual_family, ResourceVisualFamily::Generic);
                        continue;
                    }
                    let tile = chunk
                        .tiles
                        .iter()
                        .find(|tile| tile.tile == resource.node.tile)
                        .unwrap();
                    let fallback = match tile.terrain.biome {
                        Biome::Boreal => ResourceVisualFamily::Conifer,
                        Biome::Tropical => ResourceVisualFamily::Tropical,
                        Biome::Woodland | Biome::Savanna => ResourceVisualFamily::DryScrub,
                        _ => ResourceVisualFamily::Broadleaf,
                    };
                    let expected = match class {
                        Some(8 | 15 | 17) => ResourceVisualFamily::Conifer,
                        Some(13) => ResourceVisualFamily::Broadleaf,
                        Some(9) => {
                            mixed.insert(resource.visual_family as u8);
                            resource.visual_family
                        }
                        _ => fallback,
                    };
                    assert_eq!(resource.visual_family, expected, "class {class:?}");
                    trees += 1;
                    conifers +=
                        usize::from(resource.visual_family == ResourceVisualFamily::Conifer);
                }
            }
        }
    }
    assert!(trees > 100, "fixture must exercise source families");
    assert!(conifers > 100);
    assert_eq!(
        mixed,
        [1, 2].into(),
        "class 9 must yield both modeled families"
    );
}

#[test]
fn conifer_classes_emit_family_two() {
    for class in [8, 15, 17] {
        let generator = classed(1, 256, Some(class));
        let chunk = scene(&generator, 0, 0);
        let trees = chunk
            .resources
            .iter()
            .filter(|resource| resource.node.object == ObjectKind::Tree)
            .count();
        assert!(trees > 10);
        let encoded = CompactChunk::encode(&chunk).unwrap();
        assert!(encoded.payload_hex.starts_with("04"));
        assert_eq!(ResourceVisualFamily::Conifer as u8, 2);
        let decoded = encoded.decode().unwrap();
        assert!(
            decoded
                .resources
                .iter()
                .filter(|resource| resource.node.object == ObjectKind::Tree)
                .all(|resource| resource.visual_family as u8 == 2)
        );
    }
}

#[test]
fn mixed_class_nine_stands_are_coherent_and_partition_independent() {
    let mixed_source = classed(3, 256, Some(9));
    let mut stands = std::collections::BTreeMap::new();
    let mut points = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            for resource in scene(&mixed_source, x, y).resources {
                if resource.node.object != ObjectKind::Tree {
                    continue;
                }
                let tile = resource.node.tile;
                let stand = (tile.x.div_euclid(16), tile.y.div_euclid(16));
                assert_eq!(
                    *stands.entry(stand).or_insert(resource.visual_family),
                    resource.visual_family,
                    "stand {stand:?} must have one family"
                );
                points.push(resource);
            }
        }
    }
    let families: std::collections::BTreeSet<_> =
        stands.values().map(|family| *family as u8).collect();
    assert_eq!(families, [1, 2].into(), "both modeled families must appear");
    // Point queries in reverse order reproduce the chunk partition exactly.
    for resource in points.iter().rev().step_by(7) {
        let point = mixed_source
            .landscape_point_with_cancel(resource.node.tile, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(point.resource, Some(*resource));
    }
    // A fresh generator with another seed changes stands but not node identity.
    let other = classed(4, 256, Some(9));
    let mut differs = false;
    for resource in &points {
        if let Some(found) = other
            .landscape_point_with_cancel(resource.node.tile, &|| false)
            .unwrap()
            .and_then(|point| point.resource)
        {
            differs |= found.visual_family != resource.visual_family;
        }
    }
    assert!(differs);
}

#[derive(Debug)]
struct ClassProvider {
    class: u8,
    error: Option<EnvironmentPageError>,
}
impl crate::EnvironmentPageProvider for ClassProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        assert_eq!((key.level, key.x, key.y), (0, 0, 0));
        Ok(Arc::new(match key.layer {
            crate::PageLayer::Elevation => EnvironmentPage::Elevation(ElevationPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                geographic_height_centimeters: vec![100],
            }),
            crate::PageLayer::Vegetation => EnvironmentPage::Vegetation(PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![self.class],
            }),
            crate::PageLayer::Water => EnvironmentPage::Water(WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                ocean_coverage_percent: vec![0],
                inland_coverage_percent: vec![0],
            }),
            _ => panic!("unexpected source layer"),
        }))
    }
}

fn source(class: u8, error: Option<EnvironmentPageError>) -> MapChunkGenerator {
    let mut generator = provider(None);
    generator.provider = Some(Arc::new(ClassProvider { class, error }));
    generator
}

#[test]
fn provider_source_classes_match_dense_and_failures_propagate() {
    for class in [8, 9, 13, 14] {
        let dense = classed(1, 256, Some(class));
        let provided = source(class, None);
        for (x, y) in [(0, 0), (2, 5)] {
            assert_eq!(scene(&provided, x, y), scene(&dense, x, y), "class {class}");
        }
    }
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Cancelled,
    ] {
        let failing = source(8, Some(error));
        assert_eq!(
            failing.landscape_chunk_with_cancel(0, 0, &|| false),
            Err(error)
        );
        assert_eq!(
            failing.landscape_point_with_cancel(TileCoord::new(10, 10), &|| false),
            Err(error)
        );
    }
    let mixed_source = source(8, None);
    assert_eq!(
        mixed_source.landscape_chunk_with_cancel(0, 0, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    let tree = scene(&mixed_source, 0, 0)
        .resources
        .into_iter()
        .find(|resource| resource.node.object == ObjectKind::Tree)
        .expect("source tree");
    assert_eq!(
        mixed_source.source_biome_class_with_cancel(tree.node.tile, &|| false),
        Ok(Some(8))
    );
    assert_eq!(
        mixed_source.source_biome_class_with_cancel(tree.node.tile, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    // A provider environment without a vegetation field keeps the fallback.
    let mut missing = source(8, None);
    let environment = Arc::make_mut(missing.provider_environment.as_mut().unwrap());
    environment.vegetation = None;
    assert_eq!(
        missing.source_biome_class_with_cancel(tree.node.tile, &|| false),
        Ok(None)
    );
}
