//! Bounded build queues and deterministic foundation continuation.
use super::*;
use std::collections::BTreeMap;

/// Foundations one villager may hold queued: a player at 70k-unit scale lays
/// dozens at once. Still bounded, so a client cannot grow a queue without limit.
const MAX_BUILD_QUEUE: usize = 2_048;

#[cfg(test)]
thread_local! {
    pub(super) static COMPLETION_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl GameWorld {
    /// Completed farms keep their builder across transient route/spot refusals.
    /// Spread retries over game seconds so a completion wave has bounded work.
    pub(super) fn retry_completed_farm(&mut self, worker: EntityId, farm: EntityId) -> bool {
        let Some(player) = self
            .building(farm)
            .filter(|b| b.kind == BuildingKind::Farm)
            .map(|b| b.player)
        else {
            return false;
        };
        if self
            .economy
            .builders
            .get(&worker)
            .is_some_and(|job| !job.queue.is_empty())
        {
            if !self.next_queued_build(worker) {
                self.finish_farm_builder(worker, player, farm);
            }
            return true;
        }
        let period = u64::from(self.config.tick_hz);
        if self.tick.0 % period == u64::from(worker.0) % period {
            self.finish_farm_builder(worker, player, farm);
        }
        true
    }

    pub(super) fn finish_farm_builder(
        &mut self,
        worker: EntityId,
        player: PlayerId,
        farm: EntityId,
    ) {
        self.release_builder(worker);
        if let Err(EconomyError::Movement(_)) = self.issue_farm(player, worker, farm) {
            self.economy.builders.insert(
                worker,
                BuildTask {
                    building: farm,
                    attempts: 0,
                    queue: Vec::new(),
                },
            );
        }
    }

    /// Complete every site before choosing follow-up jobs. Read the members
    /// of completed sites in builder order for deterministic reassignment.
    pub(super) fn finish_builders(
        &mut self,
        completed: &BTreeMap<EntityId, (PlayerId, BuildingKind, TileRect)>,
        touched: &mut Vec<EntityId>,
    ) {
        let mut finished = completed
            .keys()
            .filter_map(|site| {
                self.economy
                    .builders
                    .members
                    .get(site)
                    .map(|ids| (site, ids))
            })
            .flat_map(|(&site, ids)| ids.iter().map(move |&id| (id, site)))
            .collect::<Vec<_>>();
        #[cfg(test)]
        COMPLETION_VISITS.with(|visits| visits.set(visits.get() + finished.len()));
        finished.sort_unstable_by_key(|&(id, _)| id);
        let mut remaining = BTreeMap::<EntityId, usize>::new();
        let mut queued = BTreeMap::<EntityId, usize>::new();
        for &(_, building) in &finished {
            *remaining.entry(building).or_default() += 1;
        }
        for &(builder, _) in &finished {
            let targets = self.economy.builders[&builder]
                .queue
                .iter()
                .collect::<std::collections::BTreeSet<_>>();
            for target in targets {
                *queued.entry(*target).or_default() += 1;
            }
        }
        let mut staffed = std::collections::BTreeSet::new();
        for (builder, building) in finished {
            let (player, kind, footprint) = completed[&building];
            let helpers = remaining.get_mut(&building).unwrap();
            *helpers -= 1;
            // Staff only when helpers retain every queued order. A builder
            // with a unique order must carry it out before becoming a farmer.
            if kind == BuildingKind::Farm
                && *helpers > 0
                && self.economy.builders[&builder]
                    .queue
                    .iter()
                    .all(|target| queued[target] > 1)
                && staffed.insert(building)
                && self
                    .economy
                    .gatherers
                    .occupied(super::super::gathering::WorkTarget::Farm(building), builder)
                    == 0
            {
                let targets = self.economy.builders[&builder]
                    .queue
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>();
                for target in targets {
                    *queued.get_mut(target).unwrap() -= 1;
                }
                self.finish_farm_builder(builder, player, building);
            } else if !self.next_queued_build(builder)
                && (kind == BuildingKind::Farm
                    || self.steward_has_held_job(builder)
                    || self.economy.dispatch.lent.contains_key(&builder)
                    || !self.continue_foundation(builder, player))
            {
                self.after_building(builder, player, building, kind, footprint);
            }
            touched.push(builder);
        }
    }

    pub(in super::super) fn validate_build_queue(
        &self,
        builders: &[EntityId],
        targets: usize,
        queued: bool,
    ) -> Result<(), EconomyError> {
        if queued
            && builders.iter().any(|id| {
                self.economy
                    .builders
                    .get(id)
                    .map_or(targets.saturating_sub(1), |t| t.queue.len() + targets)
                    > MAX_BUILD_QUEUE
            })
        {
            return Err(EconomyError::OrderTooLarge);
        }
        Ok(())
    }

    pub(in super::super) fn assign_build_orders(
        &mut self,
        builders: &[EntityId],
        targets: &[EntityId],
        queued: bool,
    ) -> bool {
        let mut any = false;
        for &builder in builders {
            if queued && let Some(task) = self.economy.builders.get_mut(&builder) {
                task.queue.extend_from_slice(targets);
                self.player_override(builder);
                any = true;
            } else if self.assign_builders(&[builder], |_| targets[0])
                && let Some(task) = self.economy.builders.get_mut(&builder)
            {
                task.queue.extend_from_slice(&targets[1..]);
                any = true;
            }
        }
        any
    }

    /// Missing, cancelled or already completed queue entries are discarded.
    /// A target whose path budget is spent stays assigned for bounded retries.
    pub(super) fn next_queued_build(&mut self, builder: EntityId) -> bool {
        let queue = self
            .economy
            .builders
            .remove(&builder)
            .map_or_else(Vec::new, |t| t.queue);
        self.release_builder(builder);
        let Some(player) = self.unit(builder).map(|u| u.player) else {
            return false;
        };
        for (index, &target) in queue.iter().enumerate() {
            let Some(footprint) = self
                .building(target)
                .filter(|b| b.player == player && !b.is_complete())
                .map(|b| b.footprint)
            else {
                continue;
            };
            let walking = match self.approach_building(builder, footprint, 0) {
                Err(crate::GameWorldError::Unreachable) => continue,
                result => result.is_ok(),
            };
            self.economy.builders.insert(
                builder,
                BuildTask {
                    building: target,
                    attempts: u8::from(!walking),
                    queue: queue[index + 1..].to_vec(),
                },
            );
            if let Some(&index) = self.lookup.get(&builder) {
                self.units[index].state.task = UnitTask::Moving;
            }
            return true;
        }
        false
    }

    /// Read only the owner's live foundations, nearest footprint first.
    pub(super) fn continue_foundation(&mut self, builder: EntityId, player: PlayerId) -> bool {
        let Some(unit) = self.unit(builder) else {
            return false;
        };
        let fishing = unit.kind == UnitKind::FishingShip;
        let from = unit.position.tile_floor();
        let steward = if fishing {
            None
        } else {
            self.steward_controller(builder)
        };
        let mut candidates = self.economy.foundations.nearest(player, from);
        while let Some(target) = candidates.next(&self.economy.foundations) {
            let Some(building) = self.building(target).filter(|b| !b.is_complete()) else {
                continue;
            };
            if (building.kind == BuildingKind::FishTrap) != fishing
                || self.economy.builders.assigned_count(target)
                    >= self.automatic_builder_limit(&building)
                || steward.is_some_and(|id| {
                    !self.economy.steward_homes[&id].connects(from, building.footprint)
                })
            {
                continue;
            }
            if self.assign_builders(&[builder], |_| target) {
                return true;
            }
        }
        false
    }
}

impl GameWorld {
    /// What a builder does next: farm its farm, gather beside a new camp,
    /// continue along a palisade, or stand idle.
    pub(super) fn after_building(
        &mut self,
        builder: EntityId,
        player: PlayerId,
        id: EntityId,
        kind: BuildingKind,
        footprint: TileRect,
    ) {
        // The admitted Steward pass restores borrowed workers under its order
        // budget, before choosing a new profession or helping another site.
        if self.steward_has_held_job(builder) {
            return;
        }
        // A manual gatherer lent to construction goes back to its resource.
        if let Some((_, target)) = self.economy.dispatch.lent.remove(&builder) {
            use super::super::gathering::WorkTarget;
            let back = match target {
                WorkTarget::Farm(farm) => self.issue_farm(player, builder, farm).is_ok(),
                WorkTarget::Node(node) => self.issue_gather(player, builder, node).is_ok(),
                WorkTarget::Animal(animal) => self.begin_carcass_gather(builder, animal).is_ok(),
            };
            if back {
                return;
            }
        }
        let radius = super::super::VILLAGE_RULES.replacement_radius_tiles;
        if kind == BuildingKind::TownCenter {
            let Some(origin) = self.unit(builder).map(|u| u.position.tile_floor()) else {
                return;
            };
            let mut nodes = [
                ResourceKind::Food,
                ResourceKind::Wood,
                ResourceKind::Gold,
                ResourceKind::Stone,
            ]
            .into_iter()
            .filter_map(|resource| {
                self.nearest_resource_matching(origin, resource, radius, None, |node| {
                    self.walking_resource_reachable(builder, node.tile)
                })
            })
            .collect::<Vec<_>>();
            nodes.sort_unstable_by_key(|node| (distance_squared(origin, node.tile), node.id));
            for node in nodes {
                if self.issue_gather(player, builder, node.id).is_ok() {
                    return;
                }
            }
            return;
        }
        let wanted: &[ResourceKind] = match kind {
            BuildingKind::LumberCamp => &[ResourceKind::Wood],
            BuildingKind::MiningCamp => &[ResourceKind::Gold, ResourceKind::Stone],
            BuildingKind::Mill => &[ResourceKind::Food],
            BuildingKind::Farm => {
                self.finish_farm_builder(builder, player, id);
                return;
            }
            _ => &[],
        };
        for resource in wanted {
            if let Some(node) =
                self.nearest_resource_matching(footprint.min, *resource, radius, None, |node| {
                    self.walking_resource_reachable(builder, node.tile)
                })
                && self.issue_gather(player, builder, node.id).is_ok()
            {
                return;
            }
        }
    }
}
