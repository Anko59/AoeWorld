//! Explicit local showcase setup. All gameplay still uses the normal commands;
//! a one-Hz game clock under the server's 20-Hz ticker speeds this demo up 20x.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig};

impl GameWorld {
    /// Four-times-clock military playground; commands remain authoritative.
    pub fn military_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 5;
        let mut world = Self::new(config)?;
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(128, 128))?;
        if let Some(p) = world.economy.players.get_mut(&PlayerId(0)) {
            p.stockpile = Stockpile {
                food: 10000,
                wood: 10000,
                gold: 10000,
                stone: 10000,
            };
        }
        world.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
        for (kind, x, y) in [
            (BuildingKind::Barracks, 121, 130),
            (BuildingKind::ArcheryRange, 128, 122),
            (BuildingKind::Stable, 131, 124),
            (BuildingKind::Blacksmith, 124, 126),
            (BuildingKind::House, 123, 135),
            (BuildingKind::House, 120, 135),
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
        for (kind, x, y) in [
            (crate::UnitKind::Militia, 132, 123),
            (crate::UnitKind::Scout, 125, 125),
        ] {
            world.spawn_kind(
                PlayerId(1),
                kind,
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(x, y))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            )?;
        }
        Ok((world, primary))
    }

    pub fn feudal_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 1;
        let mut world = Self::new(config)?;
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(128, 128))?;
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.stockpile = Stockpile {
                food: 10_000,
                wood: 10_000,
                gold: 10_000,
                stone: 10_000,
            };
        }
        for (kind, x, y) in [
            (BuildingKind::LumberCamp, 134, 128),
            (BuildingKind::Mill, 128, 136),
            (BuildingKind::Barracks, 120, 128),
        ] {
            let r = kind.rules();
            let rect = TileRect::new(
                TileCoord::new(x, y),
                TileCoord::new(x + r.side, y + r.depth),
            );
            world.validate_footprint(rect)?;
            world.insert_building(PlayerId(0), kind, rect, true)?;
        }
        Ok((world, primary))
    }
}
