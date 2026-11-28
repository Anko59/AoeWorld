use super::*;
use crate::game_world::economy::placement::{adjacent, perimeter, position};
use crate::game_world::orders::GoalPrecision;
use crate::{
    BuildingKind, CarriedResource, EconomyError, GameBuilding, GameUnit, TileVisibility, UnitTask,
};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, PlayerId};
use aoe_map::ResourceKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// 30 gold per game minute per relic, in whole gold payments.
const GOLD_INTERVAL_SECONDS: u64 = 2;

/// Number of relics that fit in one DE Monastery (DAT unit 104).
/// Record threshold: unit
const MONASTERY_RELIC_CAPACITY: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct MonasteryStore {
    pub(super) relics: BTreeMap<EntityId, u64>,
}

impl MonasteryStore {
    fn has_space(site: &Self) -> bool {
        site.relics.len() < MONASTERY_RELIC_CAPACITY
    }

    pub(super) fn valid(site: &Self) -> bool {
        !site.relics.is_empty() && site.relics.len() <= MONASTERY_RELIC_CAPACITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum RelicTarget {
    Pickup(EntityId),
    Deposit(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::game_world) struct RelicOrder {
    pub(super) target: RelicTarget,
    pub(super) approach: Option<WorldPosition>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Relics {
    pub(super) ground: BTreeMap<EntityId, WorldPosition>,
    pub(super) carriers: BTreeMap<EntityId, EntityId>,
    pub(super) stored: BTreeMap<EntityId, MonasteryStore>,
    pub(in crate::game_world) orders: BTreeMap<EntityId, RelicOrder>,
    pub(super) victory_enabled: bool,
    pub(super) victory: Option<RelicVictory>,
    #[serde(skip)]
    payments: BTreeSet<(u64, EntityId, EntityId)>,
}

impl Relics {
    pub(super) fn is_empty(&self) -> bool {
        self.ground.is_empty() && self.carriers.is_empty() && self.stored.is_empty()
    }

    pub(crate) fn retime(&mut self, at: impl Fn(u64) -> u64) {
        for relics in self.stored.values_mut() {
            for due in relics.relics.values_mut() {
                *due = at(*due);
            }
        }
        if let Some(RelicVictory::Holding { deadline, .. }) = &mut self.victory {
            deadline.0 = at(deadline.0);
        }
        self.rebuild_payments();
    }

    pub(super) fn rebuild_payments(&mut self) {
        self.payments = self
            .stored
            .iter()
            .flat_map(|(&building, relics)| {
                relics
                    .relics
                    .iter()
                    .map(move |(&relic, &due)| (due, building, relic))
            })
            .collect();
    }
}

impl GameWorld {
    pub fn carried_relic(&self, monk: EntityId) -> Option<EntityId> {
        self.relics.carriers.get(&monk).copied()
    }

    pub fn relic_carriers(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.relics.carriers.keys().copied()
    }

    pub fn monastery_relic_count(&self, monastery: EntityId) -> u16 {
        self.relics
            .stored
            .get(&monastery)
            .map_or(0, |r| r.relics.len() as u16)
    }

    pub fn monastery_has_relic_space(&self, monastery: EntityId) -> bool {
        self.relics
            .stored
            .get(&monastery)
            .is_none_or(MonasteryStore::has_space)
    }

    pub fn relic_monasteries(&self) -> impl Iterator<Item = (EntityId, u16)> + '_ {
        self.relics
            .stored
            .iter()
            .map(|(&id, r)| (id, r.relics.len() as u16))
    }

    fn relic_monk(&self, player: PlayerId, monk: EntityId) -> Result<GameUnit, EconomyError> {
        let unit = self
            .unit(monk)
            .filter(|u| u.hp > 0 && !self.embarked(monk))
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::Monk {
            return Err(EconomyError::WrongUnitKind);
        }
        Ok(unit)
    }

    pub fn issue_pickup_relic(
        &mut self,
        player: PlayerId,
        monk: EntityId,
        relic: EntityId,
    ) -> Result<(), EconomyError> {
        self.pickup_relic(player, monk, relic, true)
    }

    pub(super) fn pickup_relic(
        &mut self,
        player: PlayerId,
        monk: EntityId,
        relic: EntityId,
        require_sight: bool,
    ) -> Result<(), EconomyError> {
        let unit = self.relic_monk(player, monk)?;
        if unit.holds_relic {
            return Err(EconomyError::InvalidTarget);
        }
        let at = self
            .relics
            .ground
            .get(&relic)
            .copied()
            .filter(|at| {
                !require_sight
                    || self.visibility(player, at.tile_floor()) == TileVisibility::Visible
            })
            .ok_or(EconomyError::InvalidTarget)?;
        let target = RelicTarget::Pickup(relic);
        if self
            .relics
            .orders
            .get(&monk)
            .is_some_and(|order| order.target == target)
        {
            return Ok(());
        }
        if !self.can_touch_relic(unit, at)? {
            self.issue_task_move(monk, position(at.tile_floor())?)?;
        } else {
            self.stop_movement(self.lookup[&monk]);
        }
        self.begin_relic_order(monk, target);
        Ok(())
    }

    pub fn issue_deposit_relic(
        &mut self,
        player: PlayerId,
        monk: EntityId,
        monastery: EntityId,
    ) -> Result<(), EconomyError> {
        let unit = self.relic_monk(player, monk)?;
        if !unit.holds_relic {
            return Err(EconomyError::InvalidTarget);
        }
        let building = self
            .economy
            .buildings
            .get(&monastery)
            .filter(|b| b.kind == BuildingKind::Monastery && b.is_complete())
            .ok_or(EconomyError::InvalidTarget)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !self.monastery_has_relic_space(monastery) {
            return Err(EconomyError::GarrisonFull);
        }
        let target = RelicTarget::Deposit(monastery);
        if self
            .relics
            .orders
            .get(&monk)
            .is_some_and(|order| order.target == target)
        {
            return Ok(());
        }
        let rect = building.footprint;
        if !adjacent(rect, unit.position.tile_floor()) {
            self.approach_garrison(monk, rect)?;
        } else {
            self.stop_movement(self.lookup[&monk]);
        }
        self.begin_relic_order(monk, target);
        Ok(())
    }

    pub fn issue_drop_relic(
        &mut self,
        player: PlayerId,
        monk: EntityId,
    ) -> Result<(), EconomyError> {
        let unit = self.relic_monk(player, monk)?;
        if !unit.holds_relic {
            return Err(EconomyError::InvalidTarget);
        }
        self.issue_stop(player, monk)?;
        self.drop_monks_relic(unit);
        self.set_holds_relic(monk, false);
        Ok(())
    }

    fn begin_relic_order(&mut self, monk: EntityId, target: RelicTarget) {
        self.move_queues.remove(&monk);
        self.cancel_attack(monk);
        self.cancel_garrison_order(monk);
        let approach = self.movement_order(monk).map(|order| order.destination);
        let index = self.lookup[&monk];
        self.units[index].state.task = UnitTask::Moving;
        self.relics
            .orders
            .insert(monk, RelicOrder { target, approach });
    }

    pub(crate) fn cancel_relic_order(&mut self, monk: EntityId) {
        self.relics.orders.remove(&monk);
    }

    pub(crate) fn detach_conquered_relic(&mut self, id: EntityId) -> Option<EntityId> {
        self.relics.carriers.remove(&id)
    }

    pub(crate) fn attach_conquered_relic(&mut self, id: EntityId, relic: Option<EntityId>) {
        if let Some(relic) = relic {
            self.relics.carriers.insert(id, relic);
        }
    }

    pub(crate) fn drop_monks_relic(&mut self, unit: GameUnit) {
        self.cancel_relic_order(unit.id);
        if unit.holds_relic
            && let Some(relic) = self.relics.carriers.remove(&unit.id)
        {
            self.relics.ground.insert(relic, unit.position);
        }
    }

    fn can_touch_relic(&self, unit: GameUnit, at: WorldPosition) -> Result<bool, GameWorldError> {
        let dx = i64::from(unit.position.x) - i64::from(at.x);
        let dy = i64::from(unit.position.y) - i64::from(at.y);
        if dx * dx + dy * dy > i64::from(FIXED_SUBUNITS_PER_TILE).pow(2) {
            return Ok(false);
        }
        let from = unit.position.tile_floor();
        let to = at.tile_floor();
        Ok(from == to || self.unit_step_clear(UnitKind::Monk, from, to)?)
    }

    pub(crate) fn move_carried_relic(&mut self, from: EntityId, to: EntityId) {
        if let Some(relic) = self.relics.carriers.remove(&from) {
            self.relics.carriers.insert(to, relic);
            self.set_holds_relic(to, true);
        }
    }

    fn set_holds_relic(&mut self, monk: EntityId, holds: bool) {
        let index = self.lookup[&monk];
        self.units[index].state.holds_relic = holds;
        self.update_unit_vision(monk);
    }

    pub fn issue_eject_relic(
        &mut self,
        player: PlayerId,
        monastery: EntityId,
    ) -> Result<(), EconomyError> {
        let building = self
            .economy
            .buildings
            .get(&monastery)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        let tile = perimeter(building.footprint)
            .into_iter()
            .find(|&tile| self.tile_passable(tile))
            .ok_or(GameWorldError::Unreachable)?;
        let drop = position(tile)?;
        let held = self
            .relics
            .stored
            .get_mut(&monastery)
            .ok_or(EconomyError::InvalidTarget)?;
        let (relic, due) = held.relics.pop_first().ok_or(EconomyError::InvalidTarget)?;
        self.relics.ground.insert(relic, drop);
        self.relics.payments.remove(&(due, monastery, relic));
        if held.relics.is_empty() {
            self.relics.stored.remove(&monastery);
        }
        self.update_relic_victory();
        Ok(())
    }

    pub(crate) fn release_monastery_relics(&mut self, building: &GameBuilding) {
        let Some(relics) = self.relics.stored.remove(&building.id) else {
            return;
        };
        let rect = building.footprint;
        let tiles: Vec<_> = (rect.min.y..rect.max.y)
            .flat_map(|y| (rect.min.x..rect.max.x).map(move |x| TileCoord::new(x, y)))
            .filter_map(|tile| WorldPosition::from_tile_center(tile).ok())
            .collect();
        for (i, (relic, due)) in relics.relics.into_iter().enumerate() {
            self.relics.payments.remove(&(due, building.id, relic));
            self.relics.ground.insert(relic, tiles[i % tiles.len()]);
        }
        self.update_relic_victory();
    }

    pub(crate) fn advance_relics(&mut self) {
        if !self.relics.orders.is_empty() {
            let mut orders = std::mem::take(&mut self.relics.orders);
            orders.retain(|&monk, order| self.advance_relic_order(monk, *order));
            self.relics.orders = orders;
        }
        let now = self.tick.0;
        let period = GOLD_INTERVAL_SECONDS * u64::from(self.config.tick_hz);
        while let Some(&(due, monastery, relic)) = self.relics.payments.first() {
            if due > now {
                break;
            }
            self.relics.payments.pop_first();
            let Some(held) = self.relics.stored.get_mut(&monastery) else {
                continue;
            };
            let Some(owner) = self
                .economy
                .buildings
                .get(&monastery)
                .map(|site| site.player)
            else {
                continue;
            };
            if let Some(player) = self.economy.players.get_mut(&owner) {
                player.stockpile.deposit(CarriedResource {
                    kind: ResourceKind::Gold,
                    amount: 1,
                });
            }
            if let Some(next) = now.checked_add(period) {
                held.relics.insert(relic, next);
                self.relics.payments.insert((next, monastery, relic));
            }
        }
    }

    fn advance_relic_order(&mut self, monk: EntityId, order: RelicOrder) -> bool {
        let Some(unit) = self.unit(monk).filter(|u| u.hp > 0 && !self.embarked(monk)) else {
            return false;
        };
        let waiting = || {
            order
                .approach
                .is_some_and(|at| self.moving_to_goal(monk, at, GoalPrecision::Contact))
        };
        let tile = unit.position.tile_floor();
        match order.target {
            RelicTarget::Pickup(relic) => {
                let Some(at) = self.relics.ground.get(&relic) else {
                    return false;
                };
                if self.can_touch_relic(unit, *at) != Ok(true) {
                    return waiting();
                }
                self.relics.ground.remove(&relic);
                self.relics.carriers.insert(monk, relic);
                self.set_holds_relic(monk, true);
            }
            RelicTarget::Deposit(monastery) => {
                let Some(building) = self
                    .economy
                    .buildings
                    .get(&monastery)
                    .filter(|b| b.player == unit.player)
                else {
                    return false;
                };
                if !adjacent(building.footprint, tile) {
                    return waiting();
                }
                if !self.monastery_has_relic_space(monastery) {
                    self.stop_movement(self.lookup[&monk]);
                    self.units[self.lookup[&monk]].state.task = UnitTask::Idle;
                    return false;
                }
                let Some(relic) = self.relics.carriers.remove(&monk) else {
                    return false;
                };
                let due = self.tick.0 + GOLD_INTERVAL_SECONDS * u64::from(self.config.tick_hz);
                self.relics
                    .stored
                    .entry(monastery)
                    .or_default()
                    .relics
                    .insert(relic, due);
                self.relics.payments.insert((due, monastery, relic));
                self.set_holds_relic(monk, false);
                self.update_relic_victory();
            }
        }
        self.stop_movement(self.lookup[&monk]);
        self.units[self.lookup[&monk]].state.task = UnitTask::Idle;
        false
    }
}
