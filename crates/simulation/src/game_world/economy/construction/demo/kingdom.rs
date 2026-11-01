//! Local showcase: busy farmers beside a forest, kingdom short of wood.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{CombatStance, EntityId, PlayerId, Seed, TileCoord, TileRect, WorldPosition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage,
    MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, ResourceKind, ResourceNode,
    ordered_page_root,
};

impl GameWorld {
    /// A synthetic, flat resource map on the ordinary four-times demo clock.
    /// Every worker starts on a real Farm before its Vassal becomes active.
    pub fn vassal_kingdom_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut world = kingdom_world(seed)?;
        world.config.tick_hz = 5;
        world.set_fog_disabled(true);
        let mut nodes = Vec::new();
        if let crate::Terrain::Map { generator, .. } = &world.terrain {
            for y in 3..=5 {
                for x in 3..=5 {
                    nodes.extend(
                        generator
                            .chunk(x, y)
                            .map_err(|_| EconomyError::InvalidResource)?
                            .resources
                            .into_iter()
                            .filter(|n| n.kind == ResourceKind::Wood),
                    );
                }
            }
        }
        nodes.sort_unstable_by_key(|n| {
            let dx = i64::from(n.tile.x - 144);
            let dy = i64::from(n.tile.y - 120);
            (dx * dx + dy * dy, n.id)
        });
        let tree = nodes
            .into_iter()
            .find(|node| {
                world
                    .nearest_resource_matching(
                        node.tile,
                        ResourceKind::Wood,
                        10,
                        Some(node.id),
                        |n| n.tile.x >= node.tile.x,
                    )
                    .is_some()
            })
            .ok_or(EconomyError::InvalidResource)?;
        let origin = tree.tile;
        clear_kingdom_plot(
            &mut world,
            TileRect::from_xywh(origin.x - 31, origin.y - 24, 31, 44),
        )?;
        let castle_at = TileCoord::new(origin.x - 25, origin.y - 20);
        let castle = kingdom_building(&mut world, BuildingKind::Castle, castle_at)?;
        for x in [origin.x - 29, origin.x - 26] {
            kingdom_building(
                &mut world,
                BuildingKind::House,
                TileCoord::new(x, origin.y - 15),
            )?;
        }
        kingdom_building(
            &mut world,
            BuildingKind::LumberCamp,
            TileCoord::new(origin.x - 5, origin.y - 7),
        )?;
        let mut farms = Vec::new();
        for (dx, dy) in [(-7, 0), (-16, 9), (-25, 0)] {
            let at = TileCoord::new(origin.x + dx, origin.y + dy);
            kingdom_building(&mut world, BuildingKind::Mill, at)?;
            for (x, y) in [
                (-3, -3),
                (0, -3),
                (3, -3),
                (-3, 0),
                (3, 0),
                (-3, 3),
                (0, 3),
                (3, 3),
            ] {
                if farms.len() == 20 {
                    break;
                }
                let at = TileCoord::new(at.x + x, at.y + y);
                let farm = kingdom_building(&mut world, BuildingKind::Farm, at)?;
                farms.push((farm, TileCoord::new(at.x + 1, at.y + 1)));
            }
        }
        world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        world.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        for (farm, at) in farms {
            let worker = world.spawn_villager(PlayerId(0), kingdom_position(at)?)?;
            world.issue_farm(PlayerId(0), worker, farm)?;
            // Replenish the ordinary shared per-tick path admission budget.
            world.advance();
        }
        let primary = world.spawn_kind(
            PlayerId(0),
            UnitKind::Scout,
            kingdom_position(TileCoord::new(origin.x - 15, origin.y + 4))?,
        )?;
        world.issue_stance(PlayerId(0), &[primary], CombatStance::StandGround)?;
        let footprint = world
            .building(castle)
            .ok_or(EconomyError::UnknownEntity)?
            .footprint;
        let vassal = world.spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            kingdom_position(TileCoord::new(castle_at.x - 1, castle_at.y))?,
        )?;
        world.bind_vassal(vassal, castle, footprint);
        world.issue_garrison(PlayerId(0), vassal, castle)?;
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.stockpile = Stockpile {
                food: 30_000,
                wood: 1_000,
                gold: 30_000,
                stone: 30_000,
            };
        }
        Ok((world, primary))
    }
}

fn kingdom_position(at: TileCoord) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(at).map_err(|_| EconomyError::InvalidFootprint)
}

fn kingdom_building(
    world: &mut GameWorld,
    kind: BuildingKind,
    at: TileCoord,
) -> Result<EntityId, EconomyError> {
    let rules = kind.rules();
    world.validate_building_placement(kind, at)?;
    world.insert_building(
        PlayerId(0),
        kind,
        TileRect::from_xywh(at.x, at.y, rules.side, rules.depth),
        true,
    )
}

fn clear_kingdom_plot(world: &mut GameWorld, rect: TileRect) -> Result<(), EconomyError> {
    let mut nodes: Vec<ResourceNode> = Vec::new();
    if let crate::Terrain::Map { generator, .. } = &world.terrain {
        for y in rect.min.y.div_euclid(32)..=(rect.max.y - 1).div_euclid(32) {
            for x in rect.min.x.div_euclid(32)..=(rect.max.x - 1).div_euclid(32) {
                nodes.extend(
                    generator
                        .chunk(x, y)
                        .map_err(|_| EconomyError::InvalidResource)?
                        .resources
                        .into_iter()
                        .filter(|n| rect.contains(n.tile)),
                );
            }
        }
    }
    for node in nodes {
        world
            .deplete_resource(node.id, u16::MAX)
            .map_err(|_| EconomyError::InvalidResource)?;
    }
    Ok(())
}

fn kingdom_world(seed: Seed) -> Result<GameWorld, EconomyError> {
    let fine = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&fine))
                        .map_err(|_| EconomyError::InvalidFootprint)?,
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                        .map_err(|_| EconomyError::InvalidFootprint)?,
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            seed: seed.0,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .map_err(|_| EconomyError::InvalidFootprint)?;
    GameWorld::from_prepared_map(
        package,
        vec![fine, overview],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .map_err(Into::into)
}
