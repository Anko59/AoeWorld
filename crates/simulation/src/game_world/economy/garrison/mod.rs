//! Bounded shelter orders and occupants, removed from every map/vision index.
use super::placement::{adjacent, distance_squared, perimeter, position};
use crate::{BuildingKind, EconomyError, GameUnit, GameWorld, UnitKind, UnitTask};
use aoe_core::{EntityId, PlayerId, TileRect};

mod hash;
mod release;
mod research;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GarrisonedUnit {
    pub unit: GameUnit,
    pub(crate) next_hit_tick: u64,
    pub(crate) heal_at: u64,
}

impl BuildingKind {
    pub const fn garrison_capacity(self) -> u8 {
        match self {
            Self::TownCenter => 15,
            Self::Castle => 20,
            Self::WatchTower => 5,
            _ => 0,
        }
    }
    pub const fn admits(self, unit: UnitKind) -> bool {
        !unit.is_naval()
            && self.garrison_capacity() > 0
            && (matches!(self, Self::Castle)
                || matches!(
                    unit,
                    UnitKind::Villager
                        | UnitKind::Militia
                        | UnitKind::ManAtArms
                        | UnitKind::Spearman
                        | UnitKind::Archer
                        | UnitKind::Skirmisher
                        | UnitKind::LongSwordsman
                        | UnitKind::Pikeman
                        | UnitKind::Crossbowman
                        | UnitKind::EliteSkirmisher
                ))
    }
    fn heal_seconds(self) -> u64 {
        if self == Self::Castle { 5 } else { 10 }
    }
}

impl GameWorld {
    pub fn issue_garrison(
        &mut self,
        player: PlayerId,
        unit: EntityId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let actor = self
            .unit(unit)
            .filter(|u| u.hp > 0)
            .ok_or(EconomyError::UnknownEntity)?;
        let shelter = self.building(building).ok_or(EconomyError::UnknownEntity)?;
        if actor.player != player || shelter.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !shelter.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        if !shelter.kind.admits(actor.kind) {
            return Err(EconomyError::WrongUnitKind);
        }
        let reserved = usize::from(
            self.economy
                .garrison_reservations
                .get(&building)
                .copied()
                .unwrap_or(0),
        ) - usize::from(self.economy.garrison_orders.get(&unit) == Some(&building));
        if shelter.garrison.len() + reserved >= usize::from(shelter.kind.garrison_capacity()) {
            return Err(EconomyError::GarrisonFull);
        }
        let footprint = shelter.footprint;
        if !adjacent(footprint, actor.position.tile_floor())
            && !footprint.contains(actor.position.tile_floor())
        {
            self.approach_garrison(unit, footprint)?;
        } else {
            self.stop_movement(self.lookup[&unit]);
        }
        self.cancel_auto_scout(unit);
        self.stop_gathering(unit);
        self.cancel_attack(unit);
        self.economy.builders.remove(&unit);
        self.units[self.lookup[&unit]].state.task = UnitTask::Moving;
        self.economy.garrison_orders.insert(unit, building);
        *self
            .economy
            .garrison_reservations
            .entry(building)
            .or_default() += 1;
        Ok(())
    }

    pub(crate) fn cancel_garrison_order(&mut self, id: EntityId) {
        if let Some(building) = self.economy.garrison_orders.remove(&id)
            && let Some(count) = self.economy.garrison_reservations.get_mut(&building)
        {
            *count -= 1;
            if *count == 0 {
                self.economy.garrison_reservations.remove(&building);
            }
        }
    }

    fn approach_garrison(
        &mut self,
        id: EntityId,
        footprint: TileRect,
    ) -> Result<(), crate::GameWorldError> {
        let actor = self.unit(id).ok_or(crate::GameWorldError::UnknownEntity)?;
        let mut candidates = Vec::new();
        for tile in perimeter(footprint) {
            if self.unit_tile_passable(actor.kind, tile, &|| false)?
                && !self.economy.blocked_tiles.contains(&tile)
            {
                candidates.push(tile);
            }
        }
        candidates.sort_by_key(|tile| {
            (
                distance_squared(actor.position.tile_floor(), *tile),
                tile.y,
                tile.x,
            )
        });
        let mut error = crate::GameWorldError::Unreachable;
        for tile in candidates.into_iter().take(6) {
            match self.issue_task_move(id, position(tile)?) {
                Ok(_) => return Ok(()),
                Err(failure @ crate::GameWorldError::Environment(_)) => return Err(failure),
                Err(failure) => error = failure,
            }
        }
        Err(error)
    }

    pub(crate) fn advance_garrisons(&mut self, changed: &mut Vec<EntityId>) {
        let orders: Vec<_> = self
            .economy
            .garrison_orders
            .iter()
            .map(|(&unit, &building)| (unit, building))
            .collect();
        for (id, building) in orders {
            let Some(unit) = self.unit(id).filter(|u| u.hp > 0) else {
                self.cancel_garrison_order(id);
                continue;
            };
            let Some(shelter) = self
                .building(building)
                .filter(|b| b.hp > 0 && b.is_complete() && b.player == unit.player)
            else {
                self.cancel_garrison_order(id);
                continue;
            };
            if !adjacent(shelter.footprint, unit.position.tile_floor())
                && !shelter.footprint.contains(unit.position.tile_floor())
            {
                if !unit.moving && !unit.planning {
                    self.cancel_garrison_order(id);
                }
                continue;
            }
            let dropoff = shelter.kind;
            let heal_at =
                self.tick.0 + shelter.kind.heal_seconds() * u64::from(self.config.tick_hz);
            let index = self.lookup[&id];
            let next_hit_tick = self.units[index].next_hit_tick;
            if self.issue_stop(unit.player, id).is_err() {
                self.cancel_garrison_order(id);
                continue;
            }
            let Some(mut unit) = self.unit(id) else {
                continue;
            };
            if let Some(load) = unit.carried
                && dropoff.accepts(load.kind)
                && let Some(player) = self.economy.players.get_mut(&unit.player)
            {
                player.stockpile.deposit(load);
                unit.carried = None;
            }
            let restored = unit;
            // Normalize transient map state; ejection assigns the new position.
            unit.position = aoe_core::WorldPosition::new(0, 0);
            unit.previous_position = unit.position;
            unit.facing = crate::Facing::South;
            unit.task = if unit.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
            self.remove_unit(id);
            let Some(shelter) = self.economy.buildings.get_mut(&building) else {
                self.insert_map_unit(restored, next_hit_tick);
                changed.push(id);
                continue;
            };
            let at = shelter.garrison.partition_point(|g| g.unit.id < id);
            shelter.garrison.insert(
                at,
                GarrisonedUnit {
                    unit,
                    next_hit_tick,
                    heal_at,
                },
            );
            self.economy.occupied_garrisons.insert(building);
            self.combat.tower_ready.entry(building).or_insert(0);
        }
        for id in self.economy.occupied_garrisons.clone() {
            let Some(building) = self.economy.buildings.get_mut(&id) else {
                continue;
            };
            for occupant in &mut building.garrison {
                if self.tick.0 >= occupant.heal_at {
                    occupant.unit.hp = occupant.unit.hp.saturating_add(1).min(occupant.unit.max_hp);
                    occupant.heal_at =
                        self.tick.0 + building.kind.heal_seconds() * u64::from(self.config.tick_hz);
                }
            }
        }
        // Rare blocked destruction exits retry once per second, retaining units.
        if self.tick.0.is_multiple_of(u64::from(self.config.tick_hz)) {
            let pending = std::mem::take(&mut self.economy.ejections);
            for (id, (footprint, occupants)) in pending {
                self.release_destroyed_garrison(id, footprint, occupants, changed);
            }
        }
    }
}
