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

pub(super) fn river() -> GameWorld {
    river_width(10 * 256)
}

fn river_width(width_units: u16) -> GameWorld {
    river_channels(width_units, false)
}

pub(super) fn river_channels(width_units: u16, separate_stream: bool) -> GameWorld {
    let (package, elevation, mut water) = economy::fixture::flat_fixture(true);
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
    // Same typed, ten-tile mapped channel as map's river-fish fixture. It
    // reaches beyond both map edges, so a land route cannot walk around it.
    let mut segments = vec![RiverSegment {
        start: [128 * 256, -256],
        end: [128 * 256, 258 * 256],
        width_units,
        station_start: 0,
        station_end: 259 * 256,
        source_delta: [0, 259 * 256],
        source_station_delta: 259 * 256,
    }];
    if separate_stream {
        segments.push(RiverSegment {
            start: [116 * 256, -256],
            end: [116 * 256, 258 * 256],
            width_units: 4 * 256,
            ..segments[0]
        });
    }
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
    let mut pages = elevation
        .into_iter()
        .map(EnvironmentPage::Elevation)
        .chain(water.into_iter().map(EnvironmentPage::Water))
        .map(Arc::new)
        .collect::<Vec<_>>();
    pages.push(Arc::new(EnvironmentPage::HydrologyEvidence(evidence)));
    pages.push(Arc::new(EnvironmentPage::ModernLandCover(cover)));
    let generator = MapChunkGenerator::new([91; 32], 5, 257)
        .with_page_provider(
            Ratio::new(1, 1).unwrap(),
            environment,
            Arc::new(Pages(pages)),
        )
        .unwrap();
    let mut world = GameWorld::new(WorldConfig::new(257, 257, Seed(91)).unwrap()).unwrap();
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    world
}

#[test]
fn transport_carries_a_land_army_from_the_west_bank_to_the_east_bank() {
    let mut world = river();
    let (west, west_water, east_water, east) = banks(&world);
    assert!(west.x < 128 && east.x > 128);
    let transport = ship(&mut world, UnitKind::TransportShip, west_water, 0);
    let mut army = Vec::new();
    for _ in 0..3 {
        army.push(world.spawn_militia(PlayerId(0), at(west)).unwrap());
    }
    assert_eq!(
        world.issue_move(army[0], at(west_water)),
        Err(GameWorldError::InvalidPosition)
    );
    world.issue_load(PlayerId(0), &army, transport).unwrap();
    world.issue_move(transport, at(east_water)).unwrap();
    for _ in 0..400 {
        world.advance();
        let unit = world.unit(transport).unwrap();
        assert!(
            world
                .unit_tile_passable(UnitKind::FishingShip, unit.position.tile_floor(), &|| false)
                .unwrap()
        );
        if !unit.moving && !unit.planning {
            break;
        }
    }
    assert_eq!(world.unit(transport).unwrap().position, at(east_water));
    assert_eq!(world.transport_cargo(transport), army.as_slice());
    world.issue_unload(PlayerId(0), transport, east).unwrap();
    assert!(world.transport_cargo(transport).is_empty());
    for id in army {
        let unit = world.unit(id).unwrap();
        assert_eq!(unit.position, at(east));
        assert!(unit.hp > 0 && unit.position.tile_floor().x > 128);
        assert!(world.unit_visible_to(PlayerId(0), id));
        assert!(
            world
                .query(TileRect::from_xywh(east.x, east.y, 1, 1))
                .0
                .iter()
                .any(|unit| unit.id == id)
        );
        assert_eq!(
            world.issue_move(id, at(east_water)),
            Err(GameWorldError::InvalidPosition)
        );
    }
}

#[test]
fn ships_sail_along_shallows_while_land_units_wade_across_them() {
    let mut world = river_width(2 * 256);
    // Find an unobstructed wet lane; both movement domains use these same
    // composed Ford tiles, with their original river water classification.
    let (start, end, west, east) = (30..210)
        .find_map(|y| {
            let start = TileCoord::new(128, y);
            let end = TileCoord::new(128, y + 4);
            let west = TileCoord::new(124, y);
            let east = TileCoord::new(132, y);
            let Terrain::Map { generator, .. } = world.terrain() else {
                unreachable!()
            };
            ((y..=y + 4).all(|y| {
                generator
                    .tile_at(TileCoord::new(128, y))
                    .is_some_and(|t| t.material == GroundMaterial::Ford && t.passable)
                    && generator.object_at(TileCoord::new(128, y)).is_none()
            }) && (124..=132).all(|x| world.tile_passable(TileCoord::new(x, y))))
            .then_some((start, end, west, east))
        })
        .expect("clear wadeable stream");
    let boat = ship(&mut world, UnitKind::TransportShip, start, 0);
    let walker = world.spawn_militia(PlayerId(0), at(west)).unwrap();
    world.issue_move(boat, at(end)).unwrap();
    world.issue_move(walker, at(east)).unwrap();
    advance(&mut world, 400);
    assert_eq!(world.unit(boat).unwrap().position, at(end));
    assert_eq!(world.unit(walker).unwrap().position, at(east));
}

pub(super) fn banks(world: &GameWorld) -> (TileCoord, TileCoord, TileCoord, TileCoord) {
    (20..230)
        .find_map(|y| {
            let wet = (110..146)
                .filter(|&x| {
                    let Terrain::Map { generator, .. } = world.terrain() else {
                        unreachable!()
                    };
                    generator
                        .tile_at(TileCoord::new(x, y))
                        .is_some_and(|tile| tile.water == WaterKind::River)
                })
                .collect::<Vec<_>>();
            let (&first, &last) = (wet.first()?, wet.last()?);
            let west = TileCoord::new(first - 1, y);
            let east = TileCoord::new(last + 1, y);
            (first < 128
                && last > 128
                && world.tile_passable(west)
                && world.tile_passable(east)
                && (first..=last).all(|x| {
                    world
                        .unit_tile_passable(UnitKind::TransportShip, TileCoord::new(x, y), &|| {
                            false
                        })
                        .unwrap()
                }))
            .then_some((
                west,
                TileCoord::new(first, y),
                TileCoord::new(last, y),
                east,
            ))
        })
        .expect("river needs a clear crossing between two passable dry banks")
}
