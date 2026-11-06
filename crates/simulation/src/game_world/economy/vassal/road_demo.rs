//! Local road encounter. Foundations, movement and Vassal work use normal orders.
use super::*;
use crate::{Age, Stockpile};
use aoe_core::{Seed, WorldConfig};

impl GameWorld {
    pub fn roads_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 4; // The ordinary 20-Hz server ticker shows a five-times clock.
        let mut w = Self::new(config)?;
        w.set_fog_disabled(true);
        let primary = w.initialize_village(PlayerId(0), TileCoord::new(64, 64))?;
        w.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
        w.complete_research(PlayerId(0), crate::Tech::CastleAge, &mut Vec::new());
        w.economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::NotOwner)?
            .stockpile = Stockpile {
            food: 10000,
            wood: 10000,
            gold: 10000,
            stone: 10000,
        };
        let castle = w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(90, 90, 4, 4),
            true,
        )?;
        let sites = super::plan::sites(
            &w,
            TileCoord::new(92, 92),
            PlayerId(0),
            Age::Castle,
            &Default::default(),
            None,
        );
        for site in sites
            .into_iter()
            .filter(|s| !matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge))
        {
            let (kind, tile, vertical) = (site.kind, site.tile, site.vertical);
            let (r, mut size) = (kind.rules(), (kind.rules().side, kind.rules().depth));
            if vertical {
                size = (r.depth, r.side);
            }
            w.insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(tile.x, tile.y, size.0, size.1),
                true,
            )?;
        }
        let at = |x, y| {
            WorldPosition::from_tile_center(TileCoord::new(x, y))
                .map_err(|_| EconomyError::InvalidFootprint)
        };
        for i in 0..3 {
            w.spawn_villager(PlayerId(0), at(88 + i, 96)?)?;
        }
        let v = w.spawn_kind(PlayerId(0), UnitKind::Vassal, at(89, 92)?)?;
        w.issue_garrison(PlayerId(0), v, castle)?;
        // Start development from the panel after the native art has loaded.
        for _ in 0..2 {
            w.advance();
        }
        w.issue_vassal_development(PlayerId(0), v, false)?;
        for x in 72..=100 {
            w.insert_building(
                PlayerId(0),
                BuildingKind::Road,
                TileRect::from_xywh(x, 120, 1, 1),
                true,
            )?;
        }
        for y in [120, 123] {
            w.spawn_villager(PlayerId(0), at(72, y)?)?;
        }
        Ok((w, primary))
    }
}
