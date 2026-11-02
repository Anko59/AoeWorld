//! Imperial Age showcase on a twenty-times clock (one simulation tick per game
//! second under the server's 20-Hz ticker): a Castle Age base with a University
//! and a Siege Workshop standing, a Castle Age army and a distant enemy group.
//! Research, training and all orders stay real; only the clock and the prepaid
//! Castle Age setup are explicit.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{
    CombatStance, EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition,
};

const CASTLE_TECHS: [Tech; 12] = [
    Tech::Loom,
    Tech::ManAtArms,
    Tech::LongSwordsman,
    Tech::Pikeman,
    Tech::Crossbowman,
    Tech::Forging,
    Tech::IronCasting,
    Tech::ScaleMailArmor,
    Tech::ChainMailArmor,
    Tech::Fletching,
    Tech::BodkinArrow,
    Tech::HorseCollar,
];

impl GameWorld {
    pub fn imperial_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 1;
        let mut world = Self::new(config)?;
        world.set_fog_disabled(true);
        world.initialize_village(PlayerId(0), TileCoord::new(96, 128))?;
        // The camera centres on this scout, just south of the Town Center.
        let primary = world.spawn_kind(PlayerId(0), UnitKind::Scout, tile_center(98, 134)?)?;
        world.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        world.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        for tech in CASTLE_TECHS {
            world.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.population_cap = 200;
            player.stockpile = Stockpile {
                food: 12_000,
                wood: 12_000,
                gold: 12_000,
                stone: 12_000,
            };
        }
        for (kind, x, y) in [
            (BuildingKind::University, 98, 123),
            (BuildingKind::SiegeWorkshop, 90, 121),
            (BuildingKind::Barracks, 88, 134),
            (BuildingKind::Stable, 92, 138),
            (BuildingKind::ArcheryRange, 97, 140),
            (BuildingKind::Blacksmith, 103, 138),
            (BuildingKind::Mill, 90, 128),
        ] {
            let r = kind.rules();
            world.insert_building(
                PlayerId(0),
                kind,
                TileRect::new(
                    TileCoord::new(x, y),
                    TileCoord::new(x + r.side, y + r.depth),
                ),
                true,
            )?;
        }
        for n in 0..4 {
            for (kind, x) in [
                (UnitKind::LongSwordsman, 102),
                (UnitKind::Pikeman, 106),
                (UnitKind::Crossbowman, 107),
                (UnitKind::Knight, 104),
            ] {
                world.spawn_kind(PlayerId(0), kind, tile_center(x, 128 + n * 2)?)?;
            }
        }
        for (kind, x, y) in [
            (UnitKind::BatteringRam, 101, 136),
            (UnitKind::Mangonel, 104, 136),
        ] {
            world.spawn_kind(PlayerId(0), kind, tile_center(x, y)?)?;
        }
        // A distant enemy house keeps the opponent alive for the whole take.
        world.insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(200, 128, 2, 2),
            true,
        )?;
        for n in 0..6 {
            let id = world.spawn_kind(
                PlayerId(1),
                UnitKind::Militia,
                tile_center(130, 124 + n * 3)?,
            )?;
            world.issue_stance(PlayerId(1), &[id], CombatStance::StandGround)?;
        }
        Ok((world, primary))
    }
}

fn tile_center(x: i32, y: i32) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(TileCoord::new(x, y))
        .map_err(|_| EconomyError::InvalidFootprint)
}
