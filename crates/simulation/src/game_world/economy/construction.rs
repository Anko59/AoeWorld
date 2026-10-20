//! Placing foundations, villagers building them and finishing them.
use super::{
    BuildTask, BuildingKind, ConstructionState, EconomyError, MAX_BUILD_GROUP, MAX_BUILDERS,
    MAX_ECONOMY_BUILDINGS, Stockpile, UnitKind, UnitTask,
    placement::{adjacent, distance_squared, perimeter, position},
};
use crate::{GameUnit, GameWorld};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::ResourceKind;

/// Builders give up after this many failed approaches to one foundation.
const MAX_APPROACH_ATTEMPTS: u8 = 6;
/// Perimeter tiles one approach tries before reporting failure.
const APPROACH_TILE_TRIES: usize = 6;

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
            return Err(EconomyError::Capacity);
        }
        for id in &ids {
            let unit = self.unit(*id).ok_or(EconomyError::UnknownEntity)?;
            if unit.player != player {
                return Err(EconomyError::NotOwner);
            }
            if unit.kind != UnitKind::Villager {
                return Err(EconomyError::WrongUnitKind);
            }
        }
        let others = self
            .economy
            .builders
            .keys()
            .filter(|id| !ids.contains(id))
            .count();
        if others + ids.len() > MAX_BUILDERS {
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
        if rules.age == crate::Age::Feudal && !self.is_feudal(player) {
            return Err(EconomyError::RequiresFeudalAge);
        }
        if let Some(required) = rules.requires
            && !self.buildings().any(|building| {
                building.player == player && building.kind == required && building.is_complete()
            })
        {
            return Err(EconomyError::RequirementMissing);
        }
        if self.economy.buildings.len() + segments as usize > MAX_ECONOMY_BUILDINGS {
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
    pub fn issue_build(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        kind: BuildingKind,
        origin: TileCoord,
    ) -> Result<EntityId, EconomyError> {
        let builders = self.validate_builders(player, builders)?;
        let cost = self.check_buildable(player, kind, 1)?;
        let side = kind.rules().side;
        let footprint = TileRect::new(
            origin,
            TileCoord::new(
                origin.x.saturating_add(side),
                origin.y.saturating_add(kind.rules().depth),
            ),
        );
        self.validate_building_placement(kind, origin)?;
        let id = self.insert_building(player, kind, footprint, false)?;
        self.charge(player, cost);
        if !self.assign_builders(&builders, |_| id) {
            let _ = self.issue_cancel_construction(player, id);
            return Err(crate::GameWorldError::Unreachable.into());
        }
        self.update_building_vision(id);
        self.remember_visible_buildings();
        Ok(id)
    }

    /// Send villagers to an existing unfinished foundation of `player`
    /// (right-clicking it). Nothing is charged: the cost was paid on placement.
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
        if state.is_complete() {
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
        let cost = state.kind.rules().cost;
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
        candidates.sort_by_key(|tile| (distance_squared(origin, *tile), tile.y, tile.x));
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
    pub(super) fn advance_construction(&mut self, changed: &mut Vec<GameUnit>) {
        if self.economy.builders.is_empty() {
            return;
        }
        let ids = self.economy.builders.keys().copied().collect::<Vec<_>>();
        let mut working = std::collections::BTreeMap::<EntityId, u64>::new();
        let mut touched = ids.clone();
        for id in ids {
            let Some(task) = self.economy.builders.get(&id).copied() else {
                continue;
            };
            let Some(&index) = self.lookup.get(&id) else {
                self.economy.builders.remove(&id);
                continue;
            };
            let Some(building) = self.building(task.building) else {
                self.release_builder(id);
                continue;
            };
            let (complete, footprint) = (building.is_complete(), building.footprint);
            if complete {
                self.release_builder(id);
                continue;
            }
            if self.units[index].order.is_some() || self.units[index].state.planning {
                continue;
            }
            if adjacent(footprint, self.units[index].state.position.tile_floor()) {
                self.units[index].state.task = UnitTask::Building;
                *working.entry(task.building).or_insert(0) += 1;
                continue;
            }
            if task.attempts >= MAX_APPROACH_ATTEMPTS {
                self.release_builder(id);
                continue;
            }
            let walking = self
                .approach_building(id, footprint, usize::from(task.attempts))
                .is_ok();
            if let Some(task) = self.economy.builders.get_mut(&id) {
                task.attempts += 1;
            }
            if walking {
                self.units[index].state.task = UnitTask::Moving;
            }
        }
        for (building, count) in working {
            if let Some(done) = self.add_work(building, count) {
                touched.extend(done);
            }
        }
        touched.sort_unstable();
        touched.dedup();
        changed.extend(touched.into_iter().filter_map(|id| self.unit(id)));
    }

    /// `builders + 2` work per tick (one builder adds 3, matching the base
    /// time). Returns the released builders when the building completes.
    fn add_work(&mut self, id: EntityId, builders: u64) -> Option<Vec<EntityId>> {
        let building = self.economy.buildings.get_mut(&id)?;
        let total = building.work_total.max(1);
        let max_hp = u64::from(building.max_hp);
        let earned_before = max_hp * building.work_done / total;
        building.work_done = (building.work_done + builders + 2).min(building.work_total);
        // Work adds only the hit points it earns, so combat damage persists.
        let earned = max_hp * building.work_done / total - earned_before;
        building.hp = (u64::from(building.hp) + earned).clamp(1, max_hp) as u16;
        if building.work_done < building.work_total {
            return None;
        }
        building.state = ConstructionState::Complete;
        let (kind, player, footprint) = (building.kind, building.player, building.footprint);
        if kind == BuildingKind::WatchTower {
            self.combat.tower_ready.insert(id, 0);
        }
        self.refresh_population_cap(player);
        let finished = self
            .economy
            .builders
            .iter()
            .filter(|(_, task)| task.building == id)
            .map(|(builder, _)| *builder)
            .collect::<Vec<_>>();
        for &builder in &finished {
            self.release_builder(builder);
            self.after_building(builder, player, id, kind, footprint);
        }
        Some(finished)
    }

    /// What a builder does next: farm its farm, gather beside a new camp,
    /// continue along a palisade, or stand idle.
    fn after_building(
        &mut self,
        builder: EntityId,
        player: PlayerId,
        id: EntityId,
        kind: BuildingKind,
        footprint: TileRect,
    ) {
        let radius = super::VILLAGE_RULES.replacement_radius_tiles;
        let wanted: &[ResourceKind] = match kind {
            BuildingKind::LumberCamp => &[ResourceKind::Wood],
            BuildingKind::MiningCamp => &[ResourceKind::Gold, ResourceKind::Stone],
            BuildingKind::Mill => &[ResourceKind::Food],
            BuildingKind::Farm => {
                let _ = self.issue_farm(player, builder, id);
                return;
            }
            BuildingKind::PalisadeWall => {
                self.continue_wall(builder, player, footprint);
                return;
            }
            _ => &[],
        };
        for resource in wanted {
            if let Some(node) = self.nearest_resource(footprint.min, *resource, radius, None)
                && self.issue_gather(player, builder, node.id).is_ok()
            {
                return;
            }
        }
    }
}
