//! Portal guidance must also serve near goals behind a distant river crossing.
use crate::*;
use aoe_core::TileCoord;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
struct Pages(BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);
impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        _: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        self.0
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

fn river() -> MapChunkGenerator {
    let pyramid = || FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: [7; 32],
        }],
    };
    let segment = |y0: i32, y1: i32, width: u16| RiverSegment {
        start: [512 * 256, y0 * 256],
        end: [512 * 256, y1 * 256],
        width_units: width,
        station_start: 0,
        station_end: i64::from(y1 - y0) * 256,
        source_delta: [0, (y1 - y0) * 256],
        source_station_delta: i64::from(y1 - y0) * 256,
    };
    let evidence = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::NoEvidence as u8; 4],
        method: vec![0; 4],
        water_model: None,
        river_segments: vec![
            segment(-1, 56, 1024),
            segment(56, 72, 256),
            segment(72, 1025, 1024),
        ],
    };
    let root = ordered_hydrology_page_root(std::slice::from_ref(&evidence)).unwrap();
    let pages = vec![
        EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        }),
        EnvironmentPage::Water(WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            ocean_coverage_percent: vec![0],
            inland_coverage_percent: vec![0],
        }),
        EnvironmentPage::Vegetation(PotentialBiomePage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            potential_biome_class: vec![28],
        }),
        EnvironmentPage::ModernLandCover(ModernLandCoverPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            worldcover_class: vec![0; 4],
        }),
        EnvironmentPage::HydrologyEvidence(evidence),
    ];
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: pyramid(),
        water: Some(pyramid()),
        vegetation: Some(pyramid()),
        historical_land_use: None,
        hydrology_evidence: Some(HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: 64,
            world_cover_year: 2021,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: root,
            modern_land_cover_page_root: [4; 32],
            water_model: None,
        }),
    };
    MapChunkGenerator::new([91; 32], 5, 1024)
        .with_page_provider(
            Ratio::new(1, 1).unwrap(),
            environment,
            Arc::new(Pages(
                pages.into_iter().map(|p| (p.key(), Arc::new(p))).collect(),
            )),
        )
        .unwrap()
}

fn route(generator: &MapChunkGenerator, budget: u32) -> (Vec<TileCoord>, u32) {
    let overlay = ResourceOverlay::default();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let origin = TileCoord::new(508, 400);
    let destination = TileCoord::new(516, 400);
    let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let mut tiles = vec![origin];
    for _ in 0..100_000 {
        let before = planner.work();
        let result = planner.poll(generator, &overlay, budget, &|| false);
        assert!(planner.work() - before <= budget);
        assert!(planner.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);
        match result {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                assert_eq!(path.tiles.first(), tiles.last());
                tiles.extend(path.tiles.into_iter().skip(1));
            }
            RoutePlannerPoll::Complete => return (tiles, planner.work()),
            other => panic!("river detour ended: {other:?}"),
        }
        // Near trips need guidance before a tile flood grows without bound.
        assert!(planner.work() < 500_000, "no useful portal guidance");
    }
    panic!("river detour did not finish")
}

#[test]
fn a_near_river_order_uses_a_ford_over_three_hundred_tiles_away() {
    let generator = river();
    let (tiles, work) = route(&generator, 4096);
    assert_eq!(tiles.last(), Some(&TileCoord::new(516, 400)));
    assert!(tiles.iter().any(|t| (t.y - 400).abs() >= 300));
    let wet = tiles
        .iter()
        .filter(|t| generator.tile_at(**t).unwrap().water != WaterKind::None)
        .collect::<Vec<_>>();
    assert!(!wet.is_empty());
    assert!(
        wet.iter()
            .all(|t| generator.tile_at(**t).unwrap().material == GroundMaterial::Ford)
    );
    assert!(
        tiles
            .windows(2)
            .all(|p| generator.edge_between(p[0], p[1]) == EdgePassability::Passable)
    );
    let (replayed, replay_work) = route(&generator, 127);
    assert_eq!(
        tiles, replayed,
        "cache warming and work slicing preserve the route"
    );
    assert_eq!(work, replay_work);
}

#[test]
fn a_sealed_pocket_approaches_exactly_without_flooding_the_whole_continent() {
    sealed_approach(TileCoord::new(508, 512));
}

#[test]
fn a_far_sealed_pocket_uses_its_local_proof_and_keeps_unrelated_openings() {
    sealed_approach(TileCoord::new(10, 512));
}

#[test]
fn a_blocked_goal_inside_a_chunk_local_island_approaches_without_a_continent_flood() {
    let generator =
        crate::navigation::long_routes_tests::flat_terrain_with_water_wall(4096, -1, -1);
    let mut overlay = ResourceOverlay::default();
    for rect in [
        aoe_core::TileRect::from_xywh(522, 522, 5, 1),
        aoe_core::TileRect::from_xywh(522, 526, 5, 1),
        aoe_core::TileRect::from_xywh(522, 523, 1, 3),
        aoe_core::TileRect::from_xywh(526, 523, 1, 3),
        aoe_core::TileRect::from_xywh(524, 524, 1, 1),
    ] {
        overlay.add_footprint(rect).unwrap();
    }
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let origin = TileCoord::new(520, 524);
    let mut planner = RoutePlanner::new(origin, TileCoord::new(524, 524), MAX_ROUTE_PLANNER_WORK);
    let mut end = origin;
    for _ in 0..1000 {
        let before = planner.work();
        match planner.poll(&generator, &overlay, 127, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => end = *path.tiles.last().unwrap(),
            RoutePlannerPoll::Unreachable => {
                assert_eq!(end, TileCoord::new(524, 521));
                assert_eq!(planner.certified_destination(), Some(end));
                return;
            }
            other => panic!("blocked island approach ended: {other:?}"),
        }
        assert!(planner.work() - before <= 127);
        assert!(
            planner.work() < 100_000,
            "local island flooded the continent"
        );
    }
    panic!("blocked island approach did not finish")
}

fn sealed_approach(origin: TileCoord) {
    let generator =
        crate::navigation::long_routes_tests::flat_terrain_with_water_wall(4096, -1, -1);
    let mut overlay = ResourceOverlay::default();
    for rect in [
        aoe_core::TileRect::from_xywh(510, 510, 5, 1),
        aoe_core::TileRect::from_xywh(510, 514, 5, 1),
        aoe_core::TileRect::from_xywh(510, 511, 1, 3),
        aoe_core::TileRect::from_xywh(514, 511, 1, 3),
    ] {
        overlay.add_footprint(rect).unwrap();
    }
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let mut planner = RoutePlanner::new(origin, TileCoord::new(512, 512), MAX_ROUTE_PLANNER_WORK);
    let mut end = origin;
    for _ in 0..1000 {
        match planner.poll(&generator, &overlay, 127, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                assert_eq!(path.tiles.first(), Some(&end));
                end = *path.tiles.last().unwrap();
                if planner.certified_destination().is_some() {
                    assert!(
                        planner.opening_invalidates_proof(aoe_core::TileRect::from_xywh(
                            510, 512, 1, 1
                        )),
                        "a removed ring tile must invalidate the sealed-component exclusion"
                    );
                    assert!(
                        !planner.opening_invalidates_proof(aoe_core::TileRect::from_xywh(
                            250, 512, 1, 1
                        )),
                        "openings far from the sealed pocket must keep the trip"
                    );
                }
            }
            RoutePlannerPoll::Unreachable => {
                assert_eq!(end, TileCoord::new(512, 509));
                assert_eq!(planner.certified_destination(), Some(end));
                assert!(planner.work() < 100_000);
                return;
            }
            other => panic!("sealed approach ended: {other:?}"),
        }
        assert!(
            planner.work() < 100_000,
            "sealed pocket must not flood a continent"
        );
    }
    panic!("sealed approach flooded the continent instead of using its reverse proof")
}
