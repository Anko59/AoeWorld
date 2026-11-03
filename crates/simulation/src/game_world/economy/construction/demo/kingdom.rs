//! Local showcase: busy farmers beside a forest, kingdom short of wood.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{CombatStance, EntityId, PlayerId, Seed, TileCoord, TileRect, WorldPosition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage,
    MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, ResourceKind, ResourceNode,
    WaterPage, ordered_page_root, ordered_water_page_root,
};

impl GameWorld {
    /// A synthetic, flat resource map on the ordinary four-times demo clock.
    /// Every worker starts on a real Farm before its Vassal becomes active.
    pub fn vassal_kingdom_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut world = kingdom_world(seed)?;
        world.config.tick_hz = 5;
        world.set_fog_disabled(true);
        let tree = kingdom_tree(&mut world)?;
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
        let vassal = world.spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            kingdom_position(TileCoord::new(castle_at.x - 1, castle_at.y))?,
        )?;

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

    /// Twenty paid, working Farms and a moderate Wood shortage. The owner
    /// activates management by garrisoning the waiting Vassal during capture.
    pub fn vassal_stability_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let (mut world, primary) = Self::vassal_kingdom_demo(seed)?;
        let vassal = world
            .units()
            .find(|u| u.kind == UnitKind::Vassal)
            .map(|u| u.id)
            .ok_or(EconomyError::UnknownEntity)?;
        let from = world
            .unit(vassal)
            .ok_or(EconomyError::UnknownEntity)?
            .position
            .tile_floor();
        let waiting = TileCoord::new(from.x - 5, from.y + 12);
        if !world.tile_free(waiting) {
            return Err(EconomyError::InvalidTarget);
        }
        world.issue_stop(PlayerId(0), vassal)?;
        // Initial placement keeps the original actor and its population slot.
        // Reindex its clear waiting spot before any recording session exists.
        let index = world.lookup[&vassal];
        world.units[index].state.position = kingdom_position(waiting)?;
        let state = world.units[index].state;
        world.track_unit_step(vassal, from, &state);
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.stockpile = Stockpile {
                food: 1_800,
                wood: 1_000,
                gold: 30_000,
                stone: 30_000,
            };
        }
        Ok((world, primary))
    }

    /// Forty idle workers beside known forest, on the ordinary 20 Hz clock.
    /// The owner garrisons the waiting Vassal to start the actual job pass.
    pub fn vassal_throughput_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut world = kingdom_world(seed)?;
        world.set_fog_disabled(true);
        let origin = kingdom_tree(&mut world)?.tile;
        clear_kingdom_plot(
            &mut world,
            TileRect::from_xywh(origin.x - 31, origin.y - 24, 31, 44),
        )?;
        // Ordinary starting resources form a genuine work area for forty
        // workers; sparse generated trees alone do not have forty job slots.
        if let crate::Terrain::Map { generator, overlay } = &mut world.terrain {
            for dy in (-6..=6).step_by(2) {
                for dx in (0..=6).step_by(2) {
                    let tile = TileCoord::new(origin.x + dx, origin.y + dy);
                    if let Some(node) = ResourceNode::starting(tile, ResourceKind::Wood) {
                        // Keep any natural object already occupying this tile.
                        let _ = overlay.insert_starting_resource(generator, node);
                    }
                }
            }
        }
        world
            .resource_chunks
            .get_mut()
            .map_err(|_| EconomyError::InvalidResource)?
            .clear();
        world.resource_collision_changed();
        let castle_at = TileCoord::new(origin.x - 25, origin.y - 20);
        kingdom_building(&mut world, BuildingKind::Castle, castle_at)?;
        for i in 0..6 {
            kingdom_building(
                &mut world,
                BuildingKind::House,
                TileCoord::new(origin.x - 29 + 3 * i, origin.y - 15),
            )?;
        }
        kingdom_building(
            &mut world,
            BuildingKind::LumberCamp,
            TileCoord::new(origin.x - 3, origin.y),
        )?;
        world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        world.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        let mut workers = 0;
        'workers: for y in origin.y - 3..origin.y + 9 {
            for x in origin.x - 7..origin.x - 1 {
                let tile = TileCoord::new(x, y);
                if world.tile_free(tile) {
                    world.spawn_villager(PlayerId(0), kingdom_position(tile)?)?;
                    workers += 1;
                    if workers == 40 {
                        break 'workers;
                    }
                }
            }
        }
        if workers != 40 {
            return Err(EconomyError::Capacity);
        }
        world.spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            kingdom_position(TileCoord::new(castle_at.x - 1, castle_at.y))?,
        )?;
        let primary = world.spawn_kind(
            PlayerId(0),
            UnitKind::Scout,
            kingdom_position(TileCoord::new(origin.x - 15, origin.y + 4))?,
        )?;
        world.issue_stance(PlayerId(0), &[primary], CombatStance::StandGround)?;
        world
            .economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::NotOwner)?
            .stockpile = Stockpile {
            food: 30_000,
            wood: 1_000,
            gold: 30_000,
            stone: 30_000,
        };
        Ok((world, primary))
    }
}

pub(super) fn kingdom_position(at: TileCoord) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(at).map_err(|_| EconomyError::InvalidFootprint)
}

pub(super) fn kingdom_building(
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

pub(super) fn clear_kingdom_plot(
    world: &mut GameWorld,
    rect: TileRect,
) -> Result<(), EconomyError> {
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
    kingdom_environment(seed, false)
}

pub(super) fn coastal_world(seed: Seed) -> Result<GameWorld, EconomyError> {
    kingdom_environment(seed, true)
}

fn kingdom_environment(seed: Seed, coast: bool) -> Result<GameWorld, EconomyError> {
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
    let water = if coast {
        vec![
            WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 2,
                height: 2,
                ocean_coverage_percent: vec![0, 100, 0, 100],
                inland_coverage_percent: vec![0; 4],
            },
            WaterPage {
                level: 1,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                ocean_coverage_percent: vec![50],
                inland_coverage_percent: vec![0],
            },
        ]
    } else {
        Vec::new()
    };
    let water_field = if coast {
        let mut levels = Vec::new();
        for page in &water {
            levels.push(PyramidLevel {
                samples_per_axis: u16::from(page.width),
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(page))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            });
        }
        Some(FieldPyramid { levels })
    } else {
        None
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
        water: water_field,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            seed: seed.0,
            requested_side_meters: if coast {
                15_360
            } else {
                MapRequest::default().requested_side_meters
            },
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .map_err(|_| EconomyError::InvalidFootprint)?;
    GameWorld::from_prepared_map(package, vec![fine, overview], water, Vec::new(), Vec::new())
        .map_err(Into::into)
}

fn kingdom_tree(world: &mut GameWorld) -> Result<ResourceNode, EconomyError> {
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
                .nearest_resource_matching(node.tile, ResourceKind::Wood, 10, Some(node.id), |n| {
                    n.tile.x >= node.tile.x
                })
                .is_some()
        })
        .ok_or(EconomyError::InvalidResource)?;
    Ok(tree)
}
