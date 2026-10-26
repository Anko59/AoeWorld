//! Bounded build orders and deterministic local continuation.
use super::*;

const MAX_BUILD_QUEUE: usize = 32;
const CONTINUE_RADIUS: i32 = 10;
const CONTINUE_TRIES: usize = 4;

impl GameWorld {
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
