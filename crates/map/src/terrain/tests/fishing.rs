use super::shoreline::coast;
use super::*;

#[test]
fn fish_are_deterministic_in_chunks_and_points_on_coasts_and_lakes() {
    for lake in [false, true] {
        let generator = coast(71, &|x, _| {
            let wet = if x >= 16 { 100 } else { 0 };
            if lake { (0, wet) } else { (wet, 0) }
        });
        let mut shore = 0;
        let mut deep = 0;
        for cy in 1..7 {
            for cx in 3..7 {
                let chunk = generator.chunk(cx, cy).unwrap();
                assert_eq!(chunk, generator.chunk(cx, cy).unwrap());
                for node in chunk.resources {
                    if !matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish) {
                        continue;
                    }
                    assert_eq!(generator.object_at(node.tile), Some(node));
                    assert_eq!(generator.resource_by_id(node.id), Some(node));
                    let tile = generator.tile_at(node.tile).unwrap();
                    assert!(matches!(tile.water, WaterKind::Lake | WaterKind::Ocean));
                    match node.object {
                        ObjectKind::ShoreFish => {
                            shore += 1;
                            assert_eq!(node.initial_amount, 200);
                            assert!([(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                                generator
                                    .tile_at(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                                    .is_some_and(|t| t.passable && t.water == WaterKind::None)
                            }));
                        }
                        ObjectKind::DeepFish => {
                            deep += 1;
                            assert_eq!(
                                node.initial_amount,
                                if node.visual_variant % 5 == 4 {
                                    200
                                } else {
                                    225
                                }
                            );
                            assert_eq!(tile.material, GroundMaterial::DeepWater);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        assert!(
            shore > 0 && deep > 0,
            "shore={shore}, deep={deep}, lake={lake}"
        );
    }
}

#[test]
fn narrow_lake_has_fish_accessible_from_the_bank() {
    let generator = coast(71, &|x, _| (0, if x == 16 { 100 } else { 0 }));
    let fish = inland_fish(&generator);
    assert!(
        fish.iter()
            .filter(|node| node.object == ObjectKind::ShoreFish)
            .count()
            >= 8,
        "a 96-tile stretch of narrow lake needs bank fishing: {fish:?}"
    );
}

fn river(width_tiles: u16) -> MapChunkGenerator {
    use super::shoreline::{bind, environment};
    let (mut environment, mut pages) = environment(33, 8, &|_, _| (0, 0));
    let evidence = crate::HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        kind: vec![crate::HydrologyKind::NoEvidence as u8; 1089],
        method: vec![0; 1089],
        water_model: None,
        river_segments: vec![crate::RiverSegment {
            start: [128 * 256, -256],
            end: [128 * 256, 258 * 256],
            width_units: width_tiles * 256,
            station_start: 0,
            station_end: 259 * 256,
            source_delta: [0, 259 * 256],
            source_station_delta: 259 * 256,
        }],
    };
    evidence.validate().expect("valid synthetic river");
    let root = crate::ordered_hydrology_page_root(std::slice::from_ref(&evidence)).unwrap();
    let evidence = crate::EnvironmentPage::HydrologyEvidence(evidence);
    pages.0.insert(evidence.key(), Arc::new(evidence));
    let cover = crate::EnvironmentPage::ModernLandCover(crate::ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        worldcover_class: vec![0; 1089],
    });
    pages.0.insert(cover.key(), Arc::new(cover));
    environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
        samples_per_axis: 33,
        page_samples: 64,
        world_cover_year: 2021,
        policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: root,
        modern_land_cover_page_root: [4; 32],
        water_model: None,
    });
    bind(91, environment, pages)
}

fn inland_fish(generator: &MapChunkGenerator) -> Vec<ResourceNode> {
    let mut fish = Vec::new();
    for cy in 2..5 {
        for cx in 3..5 {
            let chunk = generator.chunk(cx, cy).unwrap();
            assert_eq!(chunk, generator.chunk(cx, cy).unwrap());
            for node in chunk.resources {
                if !matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish) {
                    continue;
                }
                assert_eq!(generator.object_at(node.tile), Some(node));
                assert_eq!(generator.resource_by_id(node.id), Some(node));
                let tile = generator.tile_at(node.tile).unwrap();
                assert!(matches!(tile.water, WaterKind::Lake | WaterKind::River));
                assert_ne!(tile.material, GroundMaterial::Ford);
                if node.object == ObjectKind::ShoreFish {
                    assert_eq!(node.initial_amount, 200);
                    assert!([(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                        generator
                            .tile_at(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                            .is_some_and(|t| t.passable && t.water == WaterKind::None)
                    }));
                } else {
                    assert_eq!(
                        node.initial_amount,
                        if node.visual_variant % 5 == 4 {
                            200
                        } else {
                            225
                        }
                    );
                    assert!([(0, -1), (-1, 0), (1, 0), (0, 1)].iter().all(|(dx, dy)| {
                        generator
                            .tile_at(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                            .is_some_and(|t| t.water != WaterKind::None)
                    }));
                }
                fish.push(node);
            }
        }
    }
    fish
}

#[test]
fn narrow_river_has_deterministic_shore_fish_on_both_banks() {
    let fish = inland_fish(&river(6));
    assert_eq!(fish, inland_fish(&river(6)));
    let shore = fish
        .iter()
        .filter(|node| node.object == ObjectKind::ShoreFish)
        .collect::<Vec<_>>();
    assert!(
        shore.len() >= 8,
        "a 96-tile river needs bank fish: {fish:?}"
    );
    assert!(shore.iter().any(|node| node.tile.x < 128));
    assert!(shore.iter().any(|node| node.tile.x > 128));
}

#[test]
fn wide_river_has_deterministic_bank_and_channel_fish() {
    // Ten tiles is the package format's current maximum river width.
    let fish = inland_fish(&river(10));
    assert_eq!(fish, inland_fish(&river(10)));
    assert!(
        fish.iter().any(|node| node.object == ObjectKind::ShoreFish),
        "river banks need fish accessible to villagers: {fish:?}"
    );
    assert!(
        fish.iter().any(|node| node.object == ObjectKind::DeepFish),
        "deep river channels need fish for fishing ships: {fish:?}"
    );
}
