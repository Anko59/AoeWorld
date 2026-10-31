//! Shared deterministic prepared map pages for tests and local filming.
use super::*;

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
pub(super) fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
pub(super) fn base(name: &str) -> Fixture {
    let (original, elevation, mut water) = flat_fixture(true);
    let mut environment = original.environment;
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
    let vegetation = vec![
        PotentialBiomePage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            potential_biome_class: vec![28; 4],
        },
        PotentialBiomePage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            potential_biome_class: vec![28],
        },
    ];
    environment.vegetation = Some(FieldPyramid {
        levels: vegetation
            .iter()
            .map(|p| PyramidLevel {
                samples_per_axis: u16::from(p.width),
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(p)).unwrap(),
            })
            .collect(),
    });
    let mut hydrology = None;
    let mut cover = None;
    if matches!(name, "river" | "bridge" | "long") {
        let segment = |x, from, to| RiverSegment {
            start: [x * 256, from * 256],
            end: [x * 256, to * 256],
            width_units: 6 * 256,
            station_start: 0,
            station_end: i64::from(to - from) * 256,
            source_delta: [0, (to - from) * 256],
            source_station_delta: i64::from(to - from) * 256,
        };
        let segments = match name {
            "river" => vec![segment(88, -1, 82), segment(88, 94, 258)],
            "bridge" => vec![segment(88, -1, 258)],
            _ => [(240, 100), (510, 65), (780, 105)]
                .into_iter()
                .flat_map(|(x, y)| [segment(x + 4, -1, y), segment(x + 4, y + 16, 1200)])
                .collect(),
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
            river_segments: segments,
        };
        let land = ModernLandCoverPage {
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
            hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&evidence))
                .unwrap(),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(std::slice::from_ref(
                &land,
            ))
            .unwrap(),
            water_model: None,
        });
        hydrology = Some(evidence);
        cover = Some(land);
    }
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: if name == "long" { 2400 } else { 514 },
            compression: Ratio::new(1, 1).unwrap(),
            seed: 0,
            ..MapRequest::default()
        },
        vec![],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .unwrap();
    let pages = elevation
        .iter()
        .cloned()
        .map(EnvironmentPage::Elevation)
        .chain(water.iter().cloned().map(EnvironmentPage::Water))
        .chain(vegetation.iter().cloned().map(EnvironmentPage::Vegetation))
        .chain(
            hydrology
                .iter()
                .cloned()
                .map(EnvironmentPage::HydrologyEvidence),
        )
        .chain(cover.iter().cloned().map(EnvironmentPage::ModernLandCover))
        .map(Arc::new)
        .collect();
    let generator = package
        .generator_with_page_provider(Arc::new(Pages(pages)))
        .unwrap();
    let side = package.estimate.tiles_per_side as i32;
    let mut world = GameWorld::new(WorldConfig::new(side, side, Seed(0)).unwrap()).unwrap();
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    world.set_fog_disabled(true);
    Fixture {
        world,
        primary: EntityId(0),
        package,
        elevation,
        water,
        vegetation,
        hydrology,
        cover,
        ids: Vec::new(),
        commands: Vec::new(),
    }
}
pub(super) fn trees(w: &mut GameWorld, rect: TileRect) {
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                overlay
                    .insert_starting_resource(
                        generator,
                        ResourceNode::starting(TileCoord::new(x, y), ResourceKind::Wood).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    w.collision_closed(rect);
}
pub(super) fn mixed(w: &mut GameWorld, x: i32, y: i32, count: usize) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            let kind = match n % 10 {
                0..=3 => UnitKind::Militia,
                4..=6 => UnitKind::Archer,
                7..=8 => UnitKind::Scout,
                _ => UnitKind::BatteringRam,
            };
            w.spawn_kind(
                PlayerId(0),
                kind,
                at(x + (n % 8) as i32 * 2, y + (n / 8) as i32 * 2),
            )
            .unwrap()
        })
        .collect()
}
