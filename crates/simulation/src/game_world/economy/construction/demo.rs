//! Explicit local showcase setup. All gameplay still uses the normal commands;
//! a one-Hz game clock under the server's 20-Hz ticker speeds this demo up 20x.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig};

impl GameWorld {
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
