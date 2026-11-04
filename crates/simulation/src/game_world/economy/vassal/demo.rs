//! Explicit headless demo fixture. Every development and combat action uses
//! ordinary authoritative orders; the four-Hz clock is shown as 5x in capture.
use super::*;
use crate::{Stockpile, Tech};
use aoe_core::{Seed, WorldConfig};
impl GameWorld {
    pub fn vassal_defense_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 4;
        let mut w = Self::new(config)?;
        w.set_fog_disabled(true);
        w.initialize_village(PlayerId(0), TileCoord::new(64, 64))?;
        let others: Vec<_> = w.units().map(|u| u.id).collect();
        for id in others {
            w.kill_unit(w.lookup[&id]);
            w.remove_unit(id);
        }
        w.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        let p = w
            .economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::NotOwner)?;
        p.stockpile = Stockpile {
            food: 20_000,
            wood: 20_000,
            gold: 20_000,
            stone: 20_000,
        };
        let at = |x, y| {
            WorldPosition::from_tile_center(TileCoord::new(x, y))
                .map_err(|_| EconomyError::InvalidFootprint)
        };
        let castle = w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(90, 90, 4, 4),
            true,
        )?;
        for i in 0..20 {
            w.spawn_villager(PlayerId(0), at(86 + i % 5, 94 + i / 5)?)?;
        }
        for i in 0..6 {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, at(96 + i % 3, 94 + i / 3)?)?;
        }
        let v = w.spawn_kind(PlayerId(0), UnitKind::Vassal, at(89, 92)?)?;
        w.issue_garrison(PlayerId(0), v, castle)?;
        for i in 0..12 {
            w.spawn_kind(PlayerId(0), UnitKind::Knight, at(138 + i % 4, 94 + i / 4)?)?;
        }
        let general = w.spawn_kind(PlayerId(0), UnitKind::General, at(138, 92)?)?;
        // An ordinary long approach leaves time to film the entire finite plan.
        // No tick-triggered purchases, damage, teleportation or injected alerts.
        for i in 0..4 {
            let enemy = w.spawn_kind(PlayerId(1), UnitKind::Knight, at(225 + i, 20)?)?;
            for leg in 0..4 {
                w.issue_queued_move(enemy, at(225 + i, if leg % 2 == 0 { 230 } else { 20 })?)?;
            }
            w.issue_queued_move(enemy, at(106, 92)?)?;
        }
        Ok((w, general))
    }
}
