//! Real-time defensive-fire encounter for native-art recording and regression QA.
use crate::{BuildingKind, EconomyError, GameWorld, Tech, UnitKind};
use aoe_core::{CombatStance, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};

impl GameWorld {
    /// Fully occupied infantry shelters face dense, stationary knight groups.
    /// Only setup is seeded: entry, targeting, flights and damage are ordinary.
    pub fn volley_demo(seed: Seed) -> Result<(Self, aoe_core::EntityId), EconomyError> {
        let config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        let mut w = Self::new(config)?;
        w.set_fog_disabled(true);
        let tc = w.place_town_center(PlayerId(0), TileCoord::new(120, 148))?;
        for tech in [Tech::FeudalAge, Tech::CastleAge] {
            w.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        let castle = w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(120, 124, 4, 4),
            true,
        )?;
        for (building, y, count) in [(castle, 124, 20), (tc, 148, 15)] {
            for _ in 0..count {
                let at = WorldPosition::from_tile_center(TileCoord::new(119, y))
                    .map_err(|_| EconomyError::InvalidFootprint)?;
                let unit = w.spawn_kind(PlayerId(0), UnitKind::Militia, at)?;
                w.issue_garrison(PlayerId(0), unit, building)?;
                w.advance();
            }
        }
        let primary = w.spawn_villager(PlayerId(0), WorldPosition::new(90 * 1024, 90 * 1024))?;
        for y in [120, 144] {
            for n in 0..48 {
                let at = WorldPosition::new(128 * 1024 + (n % 6) * 256, y * 1024 + (n / 6) * 256);
                let unit = w.spawn_kind(PlayerId(1), UnitKind::Knight, at)?;
                w.issue_stance(PlayerId(1), &[unit], CombatStance::StandGround)?;
            }
        }
        Ok((w, primary))
    }
}
