//! Bounded build orders and deterministic local continuation.
use super::*;
use std::collections::BTreeMap;

const MAX_BUILD_QUEUE: usize = 32;
const CONTINUE_RADIUS: i32 = 10;
const CONTINUE_TRIES: usize = 4;

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

    /// Complete every site before choosing follow-up jobs. One membership
    /// pass replaces a full builder scan per simultaneously completed farm.
    pub(super) fn finish_builders(
        &mut self,
        completed: &BTreeMap<EntityId, (PlayerId, BuildingKind, TileRect)>,
        touched: &mut Vec<EntityId>,
    ) {
        let finished = self
            .economy
            .builders
            .iter()
            .filter_map(|(id, job)| {
                #[cfg(test)]
                COMPLETION_VISITS.with(|visits| visits.set(visits.get() + 1));
                completed
                    .contains_key(&job.building)
                    .then_some((*id, job.building))
            })
            .collect::<Vec<_>>();
        for (builder, building) in finished {
            let (player, kind, footprint) = completed[&building];
            if !self.next_queued_build(builder)
                && (kind == BuildingKind::Farm || !self.continue_foundation(builder, player))
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
            let walking = self.approach_building(builder, footprint, 0).is_ok();
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

    /// Search only the owner's indexed buildings. Distance is to the nearest
    /// occupied footprint tile; equal distances resolve by entity ID.
    pub(super) fn continue_foundation(&mut self, builder: EntityId, player: PlayerId) -> bool {
        let Some(tile) = self.unit(builder).map(|u| u.position.tile_floor()) else {
            return false;
        };
        let mut candidates = self
            .economy
            .dropoffs
            .range((player, BuildingKind::TownCenter)..)
            .take_while(|((owner, _), _)| *owner == player)
            .flat_map(|(_, ids)| ids)
            .filter_map(|id| self.building(*id))
            .filter(|b| !b.is_complete())
            .filter_map(|b| {
                let nearest = TileCoord::new(
                    tile.x.clamp(b.footprint.min.x, b.footprint.max.x - 1),
                    tile.y.clamp(b.footprint.min.y, b.footprint.max.y - 1),
                );
                let distance = distance_squared(tile, nearest);
                (distance <= (CONTINUE_RADIUS as u64).pow(2)).then_some((
                    distance,
                    b.id,
                    b.footprint,
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|(distance, id, _)| (*distance, *id));
        let nearest = candidates.first().map(|(_, id, _)| *id);
        for (_, target, _) in candidates.into_iter().take(CONTINUE_TRIES) {
            if self.assign_builders(&[builder], |_| target) {
                return true;
            }
        }
        if let Some(target) = nearest {
            self.economy.builders.insert(
                builder,
                BuildTask {
                    building: target,
                    attempts: 1,
                    queue: Vec::new(),
                },
            );
            return true;
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
        let radius = super::super::VILLAGE_RULES.replacement_radius_tiles;
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
            if let Some(node) = self.nearest_resource(footprint.min, *resource, radius, None)
                && self.issue_gather(player, builder, node.id).is_ok()
            {
                return;
            }
        }
    }
}
