//! Local walking-animation scene: a mixed crowd, open ground and a wall with
//! a one-tile gap. Ordinary move orders drive every unit.
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};

impl GameWorld {
    /// Villagers, infantry, cavalry and a ram stand west of a palisade wall.
    pub fn walk_crowd_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        let mut world = Self::new(config)?;
        world.set_fog_disabled(true);
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(80, 128))?;
        for y in 116..=140 {
            if y != 128 {
                let rect = TileRect::from_xywh(116, y, 1, 1);
                world.insert_building(PlayerId(0), BuildingKind::PalisadeWall, rect, true)?;
            }
        }
        for (kind, x, count) in [
            (UnitKind::Villager, 98, 8),
            (UnitKind::Militia, 101, 8),
            (UnitKind::Knight, 104, 6),
            (UnitKind::BatteringRam, 108, 3),
        ] {
            for n in 0..count {
                let tile = TileCoord::new(x, 122 + n * 2);
                let position = WorldPosition::from_tile_center(tile)
                    .map_err(|_| EconomyError::InvalidFootprint)?;
                // A tile that terrain forbids only thins the crowd.
                let _ = world.spawn_kind(PlayerId(0), kind, position);
            }
        }
        Ok((world, primary))
    }
}

impl GameWorld {
    /// Repeatable army-order scene for a real socket, renderer and input.
    pub fn army_orders_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        let mut world = Self::new(config)?;
        world.set_fog_disabled(true);
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(80, 128))?;
        for y in 120..135 {
            for x in 98..118 {
                let at = WorldPosition::from_tile_center(TileCoord::new(x, y))
                    .map_err(|_| EconomyError::InvalidFootprint)?;
                world.spawn_kind(PlayerId(0), UnitKind::Militia, at)?;
            }
        }
        let wall = world.insert_building(
            PlayerId(1),
            BuildingKind::PalisadeWall,
            TileRect::from_xywh(123, 127, 1, 1),
            true,
        )?;
        // A damaged wall falls within the short recording.
        world
            .economy
            .buildings
            .get_mut(&wall)
            .ok_or(EconomyError::UnknownEntity)?
            .hp = 16;
        Ok((world, primary))
    }
}
