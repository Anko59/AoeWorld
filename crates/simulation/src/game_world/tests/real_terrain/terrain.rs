//! Typed hydrology fixture, composed through the production map generator.
use super::*;
use aoe_map::*;
use std::sync::Arc;

#[derive(Debug)]
struct Pages(Vec<Arc<EnvironmentPage>>);
impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .iter()
            .find(|page| page.key() == key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

pub(super) fn river(detour: bool) -> GameWorld {
    let (package, elevation, mut water) = super::super::economy::fixture::flat_fixture(true);
    let mut environment = package.environment;
    for page in &mut water {
        page.ocean_coverage_percent.fill(0);
    }
    for (level, page) in environment
        .water
        .as_mut()
        .unwrap()
        .levels
        .iter_mut()
        .zip(&water)
    {
        level.ordered_page_root = ordered_water_page_root(std::slice::from_ref(page)).unwrap();
    }
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        potential_biome_class: vec![28; 4],
    };
    let overview = PotentialBiomePage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![28],
    };
    environment.vegetation = Some(FieldPyramid {
        levels: vec![
            PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&biome)).unwrap(),
            },
            PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&overview))
                    .unwrap(),
            },
        ],
    });
    let segment = |from, to| RiverSegment {
        start: [88 * 256, from * 256],
        end: [88 * 256, to * 256],
        width_units: 6 * 256,
        station_start: 0,
        station_end: i64::from(to - from) * 256,
        source_delta: [0, (to - from) * 256],
        source_station_delta: i64::from(to - from) * 256,
    };
    let evidence = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        kind: vec![HydrologyKind::NoEvidence as u8; 1089],
        method: vec![0; 1089],
        water_model: None,
        river_segments: if detour {
            vec![segment(-1, 82), segment(94, 258)]
        } else {
            vec![segment(-1, 258)]
        },
    };
    let cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        worldcover_class: vec![0; 1089],
    };
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 33,
        page_samples: 64,
        world_cover_year: 2021,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&evidence)).unwrap(),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(std::slice::from_ref(
            &cover,
        ))
        .unwrap(),
        water_model: None,
    });
    let pages = elevation
        .into_iter()
        .map(EnvironmentPage::Elevation)
        .chain(water.into_iter().map(EnvironmentPage::Water))
        .chain([
            EnvironmentPage::Vegetation(biome),
            EnvironmentPage::Vegetation(overview),
            EnvironmentPage::HydrologyEvidence(evidence),
            EnvironmentPage::ModernLandCover(cover),
        ])
        .map(Arc::new)
        .collect();
    let generator = MapChunkGenerator::new([0; 32], 0, 257)
        .with_page_provider(
            Ratio::new(1, 1).unwrap(),
            environment,
            Arc::new(Pages(pages)),
        )
        .unwrap();
    let mut w = GameWorld::new(WorldConfig::new(257, 257, Seed(0)).unwrap()).unwrap();
    w.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    w
}

#[test]
fn villagers_cross_an_off_grid_diagonal_bridge_both_ways() {
    let mut w = river(false);
    w.set_fog_disabled(true);
    let span = aoe_core::BridgeSpan::new(TileCoord::new(83, 65), TileCoord::new(92, 71)).unwrap();
    if let Terrain::Map { overlay, .. } = &mut w.terrain {
        overlay.add_bridge(span).unwrap();
    }
    w.collision_opened(span.footprint());
    let mut goals = Vec::new();
    for (from, to) in [
        (center(80, 63), center(95, 73)),
        (center(95, 73), center(80, 63)),
    ] {
        let id = w.spawn_villager(PlayerId(0), from).unwrap();
        w.issue_move(id, to).unwrap();
        goals.push((id, to));
    }
    let ideal = reference_ticks(&w, &goals);
    let result = measure(&mut w, &goals, 2_000, ideal);
    eprintln!("real-terrain diagonal-bridge {result:?}");
    result.assert_works(2);
}

#[test]
fn fractional_deck_steps_keep_body_clearance_and_reject_foundations() {
    let mut w = river(false);
    let span = aoe_core::BridgeSpan::new(TileCoord::new(83, 65), TileCoord::new(92, 71)).unwrap();
    if let Terrain::Map { overlay, .. } = &mut w.terrain {
        overlay.add_bridge(span).unwrap();
    }
    let from = WorldPosition::new(90_825, 70_645);
    let to = WorldPosition::new(from.x + 16, from.y + 16);
    assert!(
        !w.unit_step_clear(UnitKind::Villager, from.tile_floor(), to.tile_floor())
            .unwrap()
    );
    assert!(
        w.unit_position_step_clear(UnitKind::Villager, from, to)
            .unwrap()
    );
    assert!(
        !w.unit_position_step_clear(
            UnitKind::Villager,
            from,
            WorldPosition::new(to.x, to.y + 700)
        )
        .unwrap()
    );
    forest(
        &mut w,
        TileRect::from_xywh(to.tile_floor().x, to.tile_floor().y, 1, 1),
    );
    assert!(
        !w.unit_position_step_clear(UnitKind::Villager, from, to)
            .unwrap()
    );
}

#[test]
fn a_short_deck_sweep_cannot_cut_the_inner_corner_of_two_links() {
    let mut w = river(false);
    let span = aoe_core::BridgeSpan::new(TileCoord::new(83, 65), TileCoord::new(92, 71)).unwrap();
    if let Terrain::Map { overlay, .. } = &mut w.terrain {
        overlay.add_bridge(span).unwrap();
    }
    // Both ends lie on inset links, but their chord leaves the nonconvex
    // union at the horizontal/diagonal join. Endpoint clearance is not proof.
    let joint = center(88, 68);
    let a = WorldPosition::new(joint.x - 143, joint.y + 307);
    let b = WorldPosition::new(joint.x - 111, joint.y + 323);
    let mid = WorldPosition::new((a.x + b.x) / 2, (a.y + b.y) / 2);
    assert!(w.body_position_clear(UnitKind::Villager, a));
    assert!(w.body_position_clear(UnitKind::Villager, b));
    assert!(!w.body_position_clear(UnitKind::Villager, mid));
    assert!(!w.body_step_clear(UnitKind::Villager, a, b));
}

#[test]
fn a_bank_link_needs_only_the_decks_direction_mask() {
    let mut w = river(false);
    let span = aoe_core::BridgeSpan::new(TileCoord::new(84, 68), TileCoord::new(92, 68)).unwrap();
    if let Terrain::Map { overlay, .. } = &mut w.terrain {
        overlay.add_bridge(span).unwrap();
        assert_eq!(overlay.bridge_connections(TileCoord::new(84, 68)), None);
        assert!(overlay.bridge_connections(TileCoord::new(85, 68)).is_some());
    }
    let from = WorldPosition::new(85 * 1024 + 256, 68 * 1024 + 512);
    let to = WorldPosition::new(from.x + 64, from.y);
    assert!(w.body_position_clear(UnitKind::Villager, from));
    assert!(w.body_position_clear(UnitKind::Villager, to));
    assert!(w.body_step_clear(UnitKind::Villager, from, to));
}
