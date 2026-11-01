//! Placing foundations, villagers building them and finishing them.
use super::{
    BuildTask, BuildingKind, ConstructionState, EconomyError, MAX_BUILD_GROUP, Stockpile, UnitKind,
    UnitTask,
    placement::{adjacent, distance_squared, perimeter, position},
};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::ResourceKind;

/// Builders give up after this many failed approaches to one foundation.
const MAX_APPROACH_ATTEMPTS: u8 = 6;
/// Perimeter tiles one approach tries before reporting failure.
const APPROACH_TILE_TRIES: usize = 6;

mod clearance;
mod continuation;
mod repair;

#[cfg(test)]
mod tests;

fn affordable(have: Stockpile, need: Stockpile) -> bool {
    have.food >= need.food
        && have.wood >= need.wood
        && have.gold >= need.gold
        && have.stone >= need.stone
}
fn scaled(cost: Stockpile, times: u32) -> Stockpile {
    Stockpile {
        food: cost.food.saturating_mul(times),
        wood: cost.wood.saturating_mul(times),
        gold: cost.gold.saturating_mul(times),
        stone: cost.stone.saturating_mul(times),
    }
}
fn add(stock: &mut Stockpile, amount: Stockpile) {
    stock.food = stock.food.saturating_add(amount.food);
    stock.wood = stock.wood.saturating_add(amount.wood);
    stock.gold = stock.gold.saturating_add(amount.gold);
    stock.stone = stock.stone.saturating_add(amount.stone);
}

impl GameWorld {
    pub(super) fn validate_builders(
        &self,
        player: PlayerId,
        builders: &[EntityId],
    ) -> Result<Vec<EntityId>, EconomyError> {
        let mut ids = builders.to_vec();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() || ids.len() > MAX_BUILD_GROUP {
            return Err(EconomyError::OrderTooLarge);
        }
        for id in &ids {
            let unit = self
                .unit(*id)
                .filter(|u| u.hp > 0 && !self.embarked(u.id))
                .ok_or(EconomyError::UnknownEntity)?;
            if unit.player != player {
                return Err(EconomyError::NotOwner);
            }
            if unit.kind != UnitKind::Villager {
                return Err(EconomyError::WrongUnitKind);
            }
        }
        // Builders already assigned elsewhere, counted from the group alone.
        let reassigned = ids
            .iter()
            .filter(|id| self.economy.builders.contains_key(id))
            .count();
        let others = self.economy.builders.len() - reassigned;
        if others + ids.len() > self.economy.limits.builders {
            return Err(EconomyError::Capacity);
        }
        Ok(ids)
    }

    pub(super) fn check_buildable(
        &self,
        player: PlayerId,
        kind: BuildingKind,
        segments: u32,
    ) -> Result<Stockpile, EconomyError> {
        let rules = kind.rules();
        if !rules.buildable {
            return Err(EconomyError::NotBuildable);
        }
        self.require_age(player, rules.age)?;
        if let Some(required) = rules.requires
            && !self
                .player_buildings_of(player, required)
                .any(|building| building.is_complete())
        {
            return Err(EconomyError::RequirementMissing);
        }
        if self.economy.buildings.len()
            + if kind == BuildingKind::Bridge {
                1
            } else {
                segments as usize
            }
            > self.economy.limits.buildings
        {
            return Err(EconomyError::Capacity);
        }
        let cost = scaled(rules.cost, segments);
        let state = self
            .economy
            .players
            .get(&player)
            .ok_or(EconomyError::NotOwner)?;
        if !affordable(state.stockpile, cost) {
            return Err(EconomyError::InsufficientResources);
        }
        Ok(cost)
    }

    pub(super) fn charge(&mut self, player: PlayerId, cost: Stockpile) {
        if let Some(state) = self.economy.players.get_mut(&player) {
            state.stockpile.food -= cost.food;
            state.stockpile.wood -= cost.wood;
            state.stockpile.gold -= cost.gold;
            state.stockpile.stone -= cost.stone;
        }
    }

    /// Order `builders` to build `kind` with its top-left tile at `origin`.
    /// The cost is charged when the foundation is placed. Walls use
    /// [`GameWorld::issue_build_wall`].
    /// `queued` appends to each builder's active construction order.
    pub fn issue_build(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        kind: BuildingKind,
        origin: TileCoord,
        queued: bool,
    ) -> Result<EntityId, EconomyError> {
        if kind == BuildingKind::Bridge {
            return Err(EconomyError::InvalidFootprint);
        }
        let builders = self.validate_builders(player, builders)?;
        self.validate_build_queue(&builders, 1, queued)?;
        let cost = self.check_buildable(player, kind, 1)?;
        let side = kind.rules().side;
        let footprint = TileRect::new(
            origin,
            TileCoord::new(
                origin.x.saturating_add(side),
                origin.y.saturating_add(kind.rules().depth),
            ),
        );
        self.validate_explored_footprint(player, footprint)?;
        self.validate_building_placement(kind, origin)?;
        let id = self.insert_building(player, kind, footprint, false)?;
        self.charge(player, cost);
        self.step_builders_aside(&builders, id);
        if !self.assign_build_orders(&builders, &[id], queued) {
            let _ = self.issue_cancel_construction(player, id);
            return Err(crate::GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(id)
    }

    /// Resume a foundation or repair a damaged completed building of `player`.
    pub fn issue_construct(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let builders = self.validate_builders(player, builders)?;
        let state = self
            .economy
            .buildings
            .get(&building)
            .ok_or(EconomyError::UnknownEntity)?;
        if state.player != player {
            return Err(EconomyError::NotOwner);
        }
        if state.is_complete() && state.hp == state.max_hp {
            return Err(EconomyError::AlreadyComplete);
        }
        if self.assign_builders(&builders, |_| building) {
            Ok(())
        } else {
            Err(crate::GameWorldError::Unreachable.into())
        }
    }

    /// Cancel a foundation. The cost is refunded in proportion to the work
    /// still undone, so a cancel before any progress refunds all of it.
    pub fn issue_cancel_construction(
        &mut self,
        player: PlayerId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let state = self
            .economy
            .buildings
            .get(&building)
            .ok_or(EconomyError::UnknownEntity)?;
        if state.player != player {
            return Err(EconomyError::NotOwner);
        }
        if state.state == ConstructionState::Complete {
            return Err(EconomyError::AlreadyComplete);
        }
        let (done, total) = (u128::from(state.work_done), u128::from(state.work_total));
        let cost = state.construction_cost();
        let refund = |amount: u32| (u128::from(amount) * (total - done) / total.max(1)) as u32;
        let refund = Stockpile {
            food: refund(cost.food),
            wood: refund(cost.wood),
            gold: refund(cost.gold),
            stone: refund(cost.stone),
        };
        self.remove_building(building);
        if let Some(state) = self.economy.players.get_mut(&player) {
            add(&mut state.stockpile, refund);
        }
        // Their foundation is gone; the per-tick pass releases the builders.
        Ok(())
    }

    /// Send each builder toward its target foundation. `false` when none
    /// could find a route.
    pub(super) fn assign_builders(
        &mut self,
        builders: &[EntityId],
        target: impl Fn(EntityId) -> EntityId,
    ) -> bool {
        let mut any = false;
        for &builder in builders {
            let building = target(builder);
            let Some(footprint) = self.building(building).map(|b| b.footprint) else {
                continue;
            };
            // Only a builder that found a route changes jobs; a refusal
            // leaves its current gather or build task as it was.
            if self.approach_building(builder, footprint, 0).is_ok() {
                self.cancel_attack(builder);
                any = true;
                self.stop_gathering(builder);
                self.economy.builders.insert(
                    builder,
                    BuildTask {
                        building,
                        attempts: 0,
                        queue: Vec::new(),
                    },
                );
                if let Some(&index) = self.lookup.get(&builder) {
                    self.units[index].state.task = UnitTask::Moving;
                }
            }
        }
        any
    }

    /// Walk to a tile beside `footprint`. `skip` rotates the candidate order so
    /// that retries do not repeat a sealed tile that exhausted the path budget.
    pub(super) fn approach_building(
        &mut self,
        id: EntityId,
        footprint: TileRect,
        skip: usize,
    ) -> Result<(), crate::GameWorldError> {
        let origin = self
            .unit(id)
            .ok_or(crate::GameWorldError::UnknownEntity)?
            .position
            .tile_floor();
        let mut candidates = perimeter(footprint);
        candidates.retain(|tile| self.tile_passable(*tile));
        candidates.sort_by_key(|tile| {
            (
                self.work_spot_busy(*tile, id),
                distance_squared(origin, *tile),
                tile.y,
                tile.x,
            )
        });
        if !candidates.is_empty() {
            let by = skip % candidates.len();
            candidates.rotate_left(by);
        }
        let mut failure = crate::GameWorldError::Unreachable;
        // A few nearest tiles only: a tile sealed inside a palisade costs a
        // full failed search, so the number of tries bounds the work.
        for tile in candidates.into_iter().take(APPROACH_TILE_TRIES) {
            match self.issue_task_move(id, position(tile)?) {
                Ok(_) => return Ok(()),
                Err(error @ crate::GameWorldError::Environment(_)) => return Err(error),
                Err(error) => failure = error,
            }
        }
        Err(failure)
    }

    fn release_builder(&mut self, id: EntityId) {
        self.economy.builders.remove(&id);
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task =
                if self.units[index].state.moving || self.units[index].state.planning {
                    UnitTask::Moving
                } else if self.units[index].state.carried.is_some() {
                    UnitTask::Carrying
                } else {
                    UnitTask::Idle
                };
        }
    }

    /// Advance every builder and foundation. Cost is bounded by the builders
    /// (at most `MAX_BUILDERS`) and, on completion, the buildings.
    pub(super) fn advance_construction(&mut self, changed: &mut Vec<EntityId>) {
        if self.economy.builders.is_empty() {
            return;
        }
        let ids = self.economy.builders.keys().copied().collect::<Vec<_>>();
        let mut working = std::collections::BTreeMap::<EntityId, u64>::new();
        let mut touched = ids.clone();
        // Radius bodies can share a tile without overlapping; work spots stay
        // one builder per perimeter tile, so arrivals skip tiles already held.
        let mut held = ids
            .iter()
            .filter_map(|id| self.unit(*id))
            .filter(|u| u.task == UnitTask::Building)
            .map(|u| u.position.tile_floor())
            .collect::<std::collections::BTreeSet<_>>();
        for id in ids {
            let Some((target, attempts)) = self
                .economy
                .builders
                .get(&id)
                .map(|t| (t.building, t.attempts))
            else {
                continue;
            };
            let Some(&index) = self.lookup.get(&id) else {
                self.economy.builders.remove(&id);
                continue;
            };
            let Some(building) = self.building(target) else {
                self.next_queued_build(id);
                continue;
            };
            let (healthy, footprint) = (
                building.is_complete() && building.hp == building.max_hp,
                building.footprint,
            );
            if healthy {
                if !self.retry_completed_farm(id, target) {
                    self.next_queued_build(id);
                }
                continue;
            }
            if self.builder_leaves_foundation(id, target) {
                continue;
            }
            // The route chooses an approach, not a mandatory work spot. Start
            // on first free contact that no other worker has reserved. Settled
            // builders retain their work spot when traffic or combat enters it.
            let tile = self.units[index].state.position.tile_floor();
            let settled = self.units[index].state.task == UnitTask::Building;
            if adjacent(footprint, tile)
                && ((self.units[index].order.is_none() && settled)
                    || ((self.units[index].order.is_none() || self.arrival_position_free(index))
                        && (settled || !held.contains(&tile))
                        && !self.work_spot_busy(tile, id)))
            {
                held.insert(tile);
                if self.units[index].order.is_some() || self.units[index].state.planning {
                    self.stop_movement(index);
                }
                self.units[index].state.face_footprint(footprint);
                self.units[index].state.task = UnitTask::Building;
                *working.entry(target).or_insert(0) += 1;
                continue;
            }
            if self.units[index].order.is_some() || self.units[index].state.planning {
                continue;
            }
            if attempts >= MAX_APPROACH_ATTEMPTS {
                self.next_queued_build(id);
                continue;
            }
            let walking = self
                .approach_building(id, footprint, usize::from(attempts))
                .is_ok();
            if let Some(task) = self.economy.builders.get_mut(&id) {
                task.attempts += 1;
            }
            if walking {
                self.units[index].state.task = UnitTask::Moving;
            } else {
                self.units[index].state.task = UnitTask::Idle;
            }
        }
        let mut completed = std::collections::BTreeMap::new();
        for (building, count) in working {
            if self.building(building).is_some_and(|b| b.is_complete()) {
                self.repair_building(building, count);
            } else if self.foundation_occupied(building) {
                // Units on the footprint are stepping aside; builders wait.
            } else if let Some(state) = self.add_work(building, count) {
                completed.insert(building, state);
            }
            self.update_building_vision(building);
        }
        if !completed.is_empty() {
            self.finish_builders(&completed, &mut touched);
        }
        touched.sort_unstable();
        touched.dedup();
        changed.extend(touched);
    }

    /// `builders + 2` work per tick (one builder adds 3, matching the base
    /// time). Completion memberships are collected once by the caller.
    fn add_work(
        &mut self,
        id: EntityId,
        builders: u64,
    ) -> Option<(PlayerId, BuildingKind, TileRect)> {
        let building = self.economy.buildings.get_mut(&id)?;
        let total = building.work_total.max(1);
        let max_hp = u64::from(building.max_hp);
        let earned_before = max_hp * building.work_done / total;
        building.work_done = (building.work_done + builders + 2).min(building.work_total);
        // Work adds only the hit points it earns, so combat damage persists.
        let earned = max_hp * building.work_done / total - earned_before;
        building.hp = (u64::from(building.hp) + earned).clamp(1, max_hp) as u16;
        if building.hp == building.max_hp {
            self.combat.hp_quarters[id.0 as usize] = 0;
        }
        if building.work_done < building.work_total {
            return None;
        }
        let (kind, player, footprint, bridge_span) = (
            building.kind,
            building.player,
            building.footprint,
            building.bridge_span,
        );
        if kind == BuildingKind::Bridge
            && bridge_span.is_none_or(|span| !self.complete_bridge(span))
        {
            if let Some(building) = self.economy.buildings.get_mut(&id) {
                building.work_done = building.work_total.saturating_sub(1);
            }
            return None;
        }
        self.economy.buildings.get_mut(&id)?.state = ConstructionState::Complete;
        self.eject_from_footprint(kind, footprint);
        if kind == BuildingKind::Farm {
            self.reset_farm_production(id);
        }
        if kind == BuildingKind::TownCenter {
            self.open_town_center_arches(footprint);
        }
        if matches!(kind, BuildingKind::WatchTower | BuildingKind::Castle) {
            self.combat.tower_ready.insert(id, 0);
        }
        Some((player, kind, footprint))
    }
}
