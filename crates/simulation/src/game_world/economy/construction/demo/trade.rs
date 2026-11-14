//! Local accelerated coast for the Trade Cart, Trade Cog and Wonder: Imperial
//! Age, two far-apart Markets and two Docks, builders and a full treasury.
use super::kingdom::{clear_kingdom_plot, coastal_world, kingdom_building, kingdom_position};
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect};

impl GameWorld {
    /// The first free site of `kind` scanning rows down from `from_y`, `xs` along each.
    fn trade_site(&self, kind: BuildingKind, xs: (i32, i32), from_y: i32) -> Option<TileCoord> {
        (from_y..self.config.height_tiles - 8)
            .flat_map(|y| (xs.0..xs.1).map(move |x| TileCoord::new(x, y)))
            .find(|&at| self.validate_building_placement(kind, at).is_ok())
    }

    pub fn trade_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut w = coastal_world(seed)?;
        w.config.tick_hz = 1;
        w.set_fog_disabled(true);
        let shore = (20..100)
            .flat_map(|y| (4..w.config.width_tiles - 4).map(move |x| TileCoord::new(x, y)))
            .find(|&at| {
                w.validate_building_placement(BuildingKind::Dock, at)
                    .is_ok()
            })
            .ok_or(EconomyError::InvalidFootprint)?;
        clear_kingdom_plot(
            &mut w,
            TileRect::from_xywh(shore.x - 32, shore.y - 4, 30, 90),
        )?;
        let tc = TileCoord::new(shore.x - 15, shore.y + 10);
        kingdom_building(&mut w, BuildingKind::TownCenter, tc)?;
        for i in 0..8 {
            kingdom_building(
                &mut w,
                BuildingKind::House,
                TileCoord::new(shore.x - 29 + 3 * i, shore.y + 2),
            )?;
        }
        for tech in [Tech::FeudalAge, Tech::CastleAge, Tech::ImperialAge] {
            w.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        let first = w
            .trade_site(BuildingKind::Market, (shore.x - 30, shore.x - 4), shore.y)
            .ok_or(EconomyError::InvalidFootprint)?;
        let second = w
            .trade_site(
                BuildingKind::Market,
                (shore.x - 30, shore.x - 4),
                first.y + 60,
            )
            .ok_or(EconomyError::InvalidFootprint)?;
        for at in [first, second] {
            kingdom_building(&mut w, BuildingKind::Market, at)?;
        }
        let dock = w
            .trade_site(BuildingKind::Dock, (4, w.config.width_tiles - 4), shore.y)
            .ok_or(EconomyError::InvalidFootprint)?;
        let far_dock = w
            .trade_site(
                BuildingKind::Dock,
                (4, w.config.width_tiles - 4),
                dock.y + 40,
            )
            .ok_or(EconomyError::InvalidFootprint)?;
        for at in [dock, far_dock] {
            kingdom_building(&mut w, BuildingKind::Dock, at)?;
        }
        let mut primary = None;
        for i in 0..6 {
            let at = TileCoord::new(tc.x - 1 + i, tc.y + 5);
            let id = w.spawn_kind(PlayerId(0), UnitKind::Villager, kingdom_position(at)?)?;
            primary.get_or_insert(id);
        }
        w.economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::NotOwner)?
            .stockpile = Stockpile {
            food: 5_000,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        };
        Ok((w, primary.ok_or(EconomyError::Capacity)?))
    }
}
