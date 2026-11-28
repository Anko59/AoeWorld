//! Compact match setup for the browser check and recorded diplomacy scene.
use super::*;

impl GameWorld {
    pub fn diplomacy_demo(seed: aoe_core::Seed) -> Result<(Self, [EntityId; 2]), EconomyError> {
        use aoe_core::{TileRect, WorldConfig, WorldPosition};
        let mut world = Self::new(
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?,
        )?;
        let human = PlayerId(0);
        let guest = PlayerId(1);
        let centre = |x, y| TileCoord::new(x, y);
        let host = world.initialize_village(human, centre(96, 101))?;
        let visitor = world.initialize_village(guest, centre(201, 201))?;
        for (player, x, y) in [(human, 88, 109), (guest, 190, 190)] {
            world.insert_building(
                player,
                BuildingKind::Market,
                TileRect::from_xywh(x, y, 4, 4),
                true,
            )?;
        }
        world.insert_building(
            human,
            BuildingKind::WatchTower,
            TileRect::from_xywh(102, 105, 1, 1),
            true,
        )?;
        let position = |x, y| {
            WorldPosition::from_tile_center(centre(x, y))
                .map_err(|_| EconomyError::InvalidFootprint)
        };
        world.spawn_militia(human, position(103, 106)?)?;
        // The visitor walks toward the tower from outside its firing range,
        // giving the two seats time to agree an alliance before they meet.
        let militia = world.spawn_militia(guest, position(118, 101)?)?;
        world.issue_move(militia, position(96, 113)?)?;
        Ok((world, [host, visitor]))
    }
}

impl GameWorld {
    pub fn diplomacy_conquest_demo(
        seed: aoe_core::Seed,
    ) -> Result<(Self, [EntityId; 2]), EconomyError> {
        use aoe_core::{TileRect, WorldConfig, WorldPosition};
        let mut world = Self::new(
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?,
        )?;
        let human = PlayerId(0);
        let ai = PlayerId(1);
        let host = world.initialize_village(human, TileCoord::new(96, 101))?;
        let visitor = world.initialize_village(ai, TileCoord::new(125, 101))?;
        for (player, x) in [(human, 95), (ai, 122)] {
            world.insert_building(
                player,
                BuildingKind::Market,
                TileRect::from_xywh(x, 111, 4, 4),
                true,
            )?;
        }
        let center = world
            .player_buildings_of(ai, BuildingKind::TownCenter)
            .next()
            .ok_or(EconomyError::UnknownEntity)?
            .id;
        world
            .building_mut(center)
            .ok_or(EconomyError::UnknownEntity)?
            .hp = 500;
        world
            .economy
            .players
            .get_mut(&ai)
            .ok_or(EconomyError::InvalidDiplomaticSeat)?
            .stockpile
            .food = 3_000;
        world.set_diplomatic_stance(human, ai, DiplomaticStance::Neutral)?;
        world.set_diplomatic_stance(ai, human, DiplomaticStance::Neutral)?;
        for y in 98..=103 {
            let at = WorldPosition::from_tile_center(TileCoord::new(116, y))
                .map_err(|_| EconomyError::InvalidFootprint)?;
            world.spawn_kind(human, crate::UnitKind::BombardCannon, at)?;
        }
        Ok((world, [host, visitor]))
    }
}
