//! Trade Carts and Trade Cogs: a trained trade unit shuttles on its own between
//! its home Market (Dock) and the farthest other own one, and is paid on arrival
//! at each end, as in AoE II DE. A manual order cancels the run (`cancel_auto_scout`
//! is the common order-cancellation hook); `issue_trade` starts it again.
//!
//! Gold per leg, from the Conquerors formula reverse-engineered in the openage
//! docs (`reverse_engineering/game_mechanics/market.md`): `2 * w * (d / size +
//! 0.3) * d + 0.5`, with `d = sqrt(max(0, dx - 5)^2 + max(0, dy - 5)^2)` the
//! centre-to-centre distance in tiles, `size` the map edge and `w` the unit's
//! work rate: 0.23 for a Cart and 0.299 for a Cog (AoE2 DE forum "An accurate
//! formula for trade", measured in the DE client). The run is not saved:
//! a restored world re-registers its trade units (`rebind_trade_units`).
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId};

/// Ticks between partner searches of a run that has no reachable partner.
const RETRY_TICKS: u64 = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TradeRun {
    /// The market or dock this leg set out from.
    pub(crate) from: EntityId,
    /// Where the unit is heading; paid on arrival. `None` while no partner exists.
    pub(crate) to: Option<EntityId>,
    /// A partner whose approach was refused, skipped by the next search.
    refused: Option<EntityId>,
}

impl UnitKind {
    pub const fn is_trader(self) -> bool {
        matches!(self, Self::TradeCart | Self::TradeCog)
    }
    /// The building kind this trader shuttles between.
    pub const fn trade_building(self) -> BuildingKind {
        if matches!(self, Self::TradeCog) {
            BuildingKind::Dock
        } else {
            BuildingKind::Market
        }
    }
    /// Work rate of the DE trade formula.
    fn trade_work_rate(self) -> f64 {
        if matches!(self, Self::TradeCog) {
            0.299
        } else {
            0.23
        }
    }
}

/// HOOK for Caravan (track B, +50% Trade Cart speed): `apply_movement_speed` in
/// `combat/military.rs` multiplies the Cart's DAT speed by this (permille) once
/// `Tech::Caravan` exists on dev. Unused until then.
#[allow(dead_code)]
pub(crate) const CARAVAN_CART_SPEED_PERMILLE: u32 = 1500;

/// Gold one leg pays. `dx2`/`dy2` are the centre offsets in half tiles.
pub(crate) fn leg_gold(kind: UnitKind, dx2: u32, dy2: u32, map_edge: i32) -> u32 {
    let reach = |half: u32| (f64::from(half) / 2.0 - 5.0).max(0.0);
    let d = (reach(dx2).powi(2) + reach(dy2).powi(2)).sqrt().max(0.1);
    let size = f64::from(map_edge.max(1));
    let gold = 2.0 * kind.trade_work_rate() * (d / size + 0.3) * d + 0.5;
    gold as u32
}

impl GameWorld {
    fn trade_centre_x2(&self, id: EntityId) -> Option<(i32, i32)> {
        let b = self.economy.buildings.get(&id)?;
        Some((
            b.footprint.min.x + b.footprint.max.x,
            b.footprint.min.y + b.footprint.max.y,
        ))
    }

    fn trade_leg_gold(&self, kind: UnitKind, a: EntityId, b: EntityId) -> u32 {
        match (self.trade_centre_x2(a), self.trade_centre_x2(b)) {
            (Some(a), Some(b)) => leg_gold(
                kind,
                a.0.abs_diff(b.0),
                a.1.abs_diff(b.1),
                self.config.width_tiles.min(self.config.height_tiles),
            ),
            _ => 0,
        }
    }

    fn trade_usable(&self, player: PlayerId, kind: BuildingKind, id: EntityId) -> bool {
        self.economy
            .buildings
            .get(&id)
            .is_some_and(|b| b.player == player && b.kind == kind && b.hp > 0 && b.is_complete())
    }

    /// The farthest other own building of the trader's kind; ties to the lowest id.
    fn trade_partner(
        &self,
        player: PlayerId,
        kind: UnitKind,
        from: EntityId,
        skip: Option<EntityId>,
    ) -> Option<EntityId> {
        let origin = self.trade_centre_x2(from)?;
        let mut best: Option<(u64, EntityId)> = None;
        for &id in self
            .economy
            .dropoffs
            .get(&(player, kind.trade_building()))
            .into_iter()
            .flatten()
        {
            if id == from
                || Some(id) == skip
                || !self.trade_usable(player, kind.trade_building(), id)
            {
                continue;
            }
            let Some(at) = self.trade_centre_x2(id) else {
                continue;
            };
            let (dx, dy) = (
                u64::from(origin.0.abs_diff(at.0)),
                u64::from(origin.1.abs_diff(at.1)),
            );
            let far = dx * dx + dy * dy;
            // Buildings that touch pay nothing: not a route.
            if far > 4 * 36 && best.is_none_or(|(f, _)| far > f) {
                best = Some((far, id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Start a freshly trained or restored trader shuttling from `home`.
    pub(crate) fn begin_trade(&mut self, unit: EntityId, home: EntityId) {
        self.economy.trade.insert(
            unit,
            TradeRun {
                from: home,
                to: None,
                refused: None,
            },
        );
    }

    /// Order a trader to trade between the named building and the farthest
    /// other own one. The building must be of the trader's kind and complete.
    pub fn issue_trade(
        &mut self,
        player: PlayerId,
        unit: EntityId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let u = self
            .unit(unit)
            .filter(|u| u.hp > 0)
            .ok_or(EconomyError::UnknownEntity)?;
        if u.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !u.kind.is_trader() {
            return Err(EconomyError::WrongUnitKind);
        }
        let b = self
            .economy
            .buildings
            .get(&building)
            .ok_or(EconomyError::UnknownEntity)?;
        if b.player != player {
            return Err(EconomyError::NotOwner);
        }
        if b.kind != u.kind.trade_building() {
            return Err(EconomyError::WrongUnitKind);
        }
        if !b.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        self.issue_stop(player, unit)?;
        self.begin_trade(unit, building);
        Ok(())
    }

    pub(crate) fn cancel_trade(&mut self, id: EntityId) {
        self.economy.trade.remove(&id);
    }

    /// A restored world has no runs: every living trader restarts from the
    /// nearest own building of its kind.
    pub(crate) fn rebind_trade_units(&mut self) {
        let traders: Vec<_> = self
            .units
            .iter()
            .filter(|u| u.state.hp > 0 && u.state.kind.is_trader())
            .map(|u| {
                (
                    u.state.id,
                    u.state.player,
                    u.state.kind,
                    u.state.position.tile_floor(),
                )
            })
            .collect();
        for (id, player, kind, at) in traders {
            let home = self
                .economy
                .dropoffs
                .get(&(player, kind.trade_building()))
                .into_iter()
                .flatten()
                .copied()
                .filter(|&b| self.trade_usable(player, kind.trade_building(), b))
                .min_by_key(|b| {
                    let c = self.trade_centre_x2(*b).unwrap_or((0, 0));
                    (
                        i64::from(c.0 - 2 * at.x).pow(2) + i64::from(c.1 - 2 * at.y).pow(2),
                        *b,
                    )
                });
            if let Some(home) = home {
                self.begin_trade(id, home);
            }
        }
    }

    pub(super) fn advance_trade(&mut self, changed: &mut Vec<EntityId>) {
        if self.economy.trade.is_empty() {
            return;
        }
        let ids: Vec<_> = self.economy.trade.keys().copied().collect();
        for id in ids {
            let Some(unit) = self.unit(id).filter(|u| u.hp > 0 && u.death_tick.is_none()) else {
                self.cancel_trade(id);
                continue;
            };
            let index = self.lookup[&id];
            if self.units[index].order.is_some() || unit.planning || unit.moving {
                continue;
            }
            let Some(mut run) = self.economy.trade.get(&id).copied() else {
                continue;
            };
            let building = unit.kind.trade_building();
            // A lost or unfinished endpoint ends the leg; look for a partner again.
            if !self.trade_usable(unit.player, building, run.from) {
                let nearest = self
                    .economy
                    .dropoffs
                    .get(&(unit.player, building))
                    .and_then(|v| {
                        v.iter()
                            .copied()
                            .find(|&b| self.trade_usable(unit.player, building, b))
                    });
                match nearest {
                    Some(b) => run.from = b,
                    None => {
                        self.cancel_trade(id);
                        continue;
                    }
                }
                run.to = None;
            }
            if run
                .to
                .is_some_and(|to| !self.trade_usable(unit.player, building, to))
            {
                run.to = None;
            }
            if run.to.is_none() {
                if !(self.tick.0 + u64::from(id.0)).is_multiple_of(RETRY_TICKS) {
                    self.economy.trade.insert(id, run);
                    continue;
                }
                run.to = self.trade_partner(unit.player, unit.kind, run.from, run.refused.take());
            }
            let Some(to) = run.to else {
                self.economy.trade.insert(id, run);
                continue;
            };
            let at = unit.position.tile_floor();
            let arrived = self
                .economy
                .buildings
                .get(&to)
                .is_some_and(|b| super::placement::adjacent(b.footprint, at));
            if arrived {
                let gold = self.trade_leg_gold(unit.kind, run.from, to);
                if let Some(state) = self.economy.players.get_mut(&unit.player) {
                    state.stockpile.gold = state.stockpile.gold.saturating_add(gold);
                }
                // The next leg starts here: back to where this one began, or
                // the farthest partner if that building is gone.
                run = TradeRun {
                    from: to,
                    to: Some(run.from),
                    refused: None,
                };
                if !self.trade_usable(unit.player, building, run.to.unwrap_or(to)) {
                    run.to = None;
                }
                changed.push(id);
            }
            if let Some(target) = run.to
                && let Some(footprint) = self.economy.buildings.get(&target).map(|b| b.footprint)
                && !super::placement::adjacent(footprint, at)
            {
                match self.approach_garrison(id, footprint) {
                    Ok(()) => {
                        self.units[index].state.task = crate::UnitTask::Moving;
                        changed.push(id);
                    }
                    // Unreachable partner: look for another one next time.
                    Err(_) => {
                        run.refused = Some(target);
                        run.to = None;
                    }
                }
            }
            self.economy.trade.insert(id, run);
        }
    }
}

impl crate::game_world::economy::Economy {
    pub(crate) fn hash_trade(&self, hash: &mut blake3::Hasher) {
        hash.update(&(self.trade.len() as u64).to_le_bytes());
        for (id, run) in &self.trade {
            hash.update(&id.0.to_le_bytes());
            hash.update(&run.from.0.to_le_bytes());
            hash.update(&run.to.map_or(u32::MAX, |e| e.0).to_le_bytes());
            hash.update(&run.refused.map_or(u32::MAX, |e| e.0).to_le_bytes());
        }
    }
}
