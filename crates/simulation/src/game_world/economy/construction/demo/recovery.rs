//! Local army recovery scenes; real game orders, bodies and building walls.
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind};
use aoe_core::{
    CombatStance, EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition,
};

fn at(x: i32, y: i32) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(TileCoord::new(x, y))
        .map_err(|_| EconomyError::InvalidFootprint)
}

impl GameWorld {
    /// Reproduce the seed-1 village's narrow annex lane. Ordinary UI move
    /// orders start the two crossing villagers; the stationary work body stays.
    pub fn movement_lane_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 5;
        let mut world = Self::new(config)?;
        world.terrain = flat_terrain(seed)?;
        world.set_fog_disabled(true);
        for (kind, x, y) in [
            (BuildingKind::TownCenter, 151, 151),
            (BuildingKind::Barracks, 147, 151),
            (BuildingKind::House, 149, 148),
            (BuildingKind::LumberCamp, 150, 156),
        ] {
            let r = kind.rules();
            world.insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(x - 22, y - 24, r.side, r.depth),
                true,
            )?;
        }
        let parked = world.spawn_villager(PlayerId(0), at(128, 128)?)?;
        world.units[world.lookup[&parked]].state.task = crate::UnitTask::Hunting;
        let primary = world.spawn_villager(PlayerId(0), WorldPosition::new(131228, 130550))?;
        world.spawn_villager(PlayerId(0), at(128, 129)?)?;
        Ok((world, primary))
    }

    /// First sight or an already admitted fight, closed by a normal palisade
    /// enclosure. The caller issues the first-sight march through ordinary UI.
    pub fn movement_recovery_demo(
        seed: Seed,
        acquired: bool,
    ) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 5;
        let mut world = Self::new(config)?;
        world.terrain = flat_terrain(seed)?;
        world.set_fog_disabled(true);
        for n in 0..100 {
            let kind = match n % 10 {
                0..=3 => UnitKind::Militia,
                4..=6 => UnitKind::Archer,
                7..=8 => UnitKind::Scout,
                _ => UnitKind::BatteringRam,
            };
            world.spawn_kind(PlayerId(0), kind, at(108 + n % 10, 125 + n / 10)?)?;
        }
        // Creation is the only census: the General appears after the army so
        // it adopts every follower, then ticks settle before the enemy arrives.
        let general = world.spawn_kind(PlayerId(0), UnitKind::General, at(128, 128)?)?;
        for _ in 0..10 {
            world.advance();
        }

        let enemy = world.spawn_kind(PlayerId(1), UnitKind::Militia, at(138, 128)?)?;
        world.issue_stance(PlayerId(1), &[enemy], CombatStance::NoAttack)?;
        if acquired {
            world.issue_move(general, at(213, 135)?)?;
            for _ in 0..10 {
                world.advance();
            }
        }
        for y in 35..=47 {
            for x in 54..=66 {
                if x == 54 || x == 66 || y == 35 || y == 47 {
                    world.insert_building(
                        PlayerId(0),
                        BuildingKind::PalisadeWall,
                        TileRect::from_xywh(x + 78, y + 87, 1, 1),
                        true,
                    )?;
                }
            }
        }
        Ok((world, general))
    }
}

// A prepared flat map gives the recording the same overlay/portal navigation
// as the terrain regressions. Uniform diagnostic ground has no portal proof.
fn flat_terrain(seed: Seed) -> Result<crate::Terrain, EconomyError> {
    use aoe_map::{
        ElevationPage, FieldPyramid, MapChunkGenerator, PreparedEnvironment, PyramidLevel, Ratio,
        ResourceOverlay, ordered_page_root,
    };
    let pages: Vec<_> = [(0, 2), (1, 1)]
        .into_iter()
        .map(|(level, side)| ElevationPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            geographic_height_centimeters: vec![0; usize::from(side) * usize::from(side)],
        })
        .collect();
    let mut environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: pages
                .iter()
                .map(|page| {
                    Ok(PyramidLevel {
                        samples_per_axis: u16::from(page.width),
                        ordered_page_root: ordered_page_root(std::slice::from_ref(page))
                            .map_err(|_| EconomyError::InvalidFootprint)?,
                    })
                })
                .collect::<Result<_, EconomyError>>()?,
        },
        ..PreparedEnvironment::default()
    };
    let water: Vec<_> = pages
        .iter()
        .map(|p| aoe_map::WaterPage {
            level: p.level,
            x: 0,
            y: 0,
            width: p.width,
            height: p.height,
            ocean_coverage_percent: vec![0; p.geographic_height_centimeters.len()],
            inland_coverage_percent: vec![0; p.geographic_height_centimeters.len()],
        })
        .collect();
    let biomes: Vec<_> = pages
        .iter()
        .map(|p| aoe_map::PotentialBiomePage {
            level: p.level,
            x: 0,
            y: 0,
            width: p.width,
            height: p.height,
            potential_biome_class: vec![28; p.geographic_height_centimeters.len()],
        })
        .collect();
    environment.water = Some(FieldPyramid {
        levels: water
            .iter()
            .map(|p| {
                Ok(PyramidLevel {
                    samples_per_axis: u16::from(p.width),
                    ordered_page_root: aoe_map::ordered_water_page_root(std::slice::from_ref(p))
                        .map_err(|_| EconomyError::InvalidFootprint)?,
                })
            })
            .collect::<Result<_, EconomyError>>()?,
    });
    environment.vegetation = Some(FieldPyramid {
        levels: biomes
            .iter()
            .map(|p| {
                Ok(PyramidLevel {
                    samples_per_axis: u16::from(p.width),
                    ordered_page_root: aoe_map::ordered_biome_page_root(std::slice::from_ref(p))
                        .map_err(|_| EconomyError::InvalidFootprint)?,
                })
            })
            .collect::<Result<_, EconomyError>>()?,
    });
    let generator = MapChunkGenerator::new([0; 32], seed.0, 256)
        .with_prepared_elevation(
            Ratio::new(1, 1).map_err(|_| EconomyError::InvalidFootprint)?,
            &environment,
            pages,
        )
        .map_err(|_| EconomyError::InvalidFootprint)?
        .with_prepared_water(&environment, water)
        .map_err(|_| EconomyError::InvalidFootprint)?
        .with_prepared_biomes(&environment, biomes)
        .map_err(|_| EconomyError::InvalidFootprint)?;
    Ok(crate::Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    })
}
