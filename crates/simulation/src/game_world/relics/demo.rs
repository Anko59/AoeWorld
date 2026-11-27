use super::*;
use crate::{BuildingKind, EconomyError, Tech};
use aoe_core::{Seed, TileRect, WorldConfig};

impl GameWorld {
    /// Small local scene for browser interaction and the in-game recording.
    pub fn relic_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let config = WorldConfig::new(128, 128, seed).map_err(GameWorldError::InvalidConfig)?;
        let mut world = Self::new(config)?;
        let owner = PlayerId(0);
        let monk = world.spawn_kind(
            owner,
            UnitKind::Monk,
            WorldPosition::from_tile_center(TileCoord::new(62, 66))
                .map_err(GameWorldError::InvalidConfig)?,
        )?;
        world.insert_building(
            owner,
            BuildingKind::Monastery,
            TileRect::from_xywh(69, 65, 3, 3),
            true,
        )?;
        for tech in [Tech::FeudalAge, Tech::CastleAge] {
            world.complete_research(owner, tech, &mut Vec::new());
        }
        let relic = world.allocate_entity_id()?;
        world.relics.ground.insert(
            relic,
            WorldPosition::from_tile_center(TileCoord::new(65, 66))
                .map_err(GameWorldError::InvalidConfig)?,
        );
        Ok((world, monk))
    }
}
