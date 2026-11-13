//! Restore pending player intent after all current jobs have been reissued.
use super::{GameWorld, UnitRecord};
use aoe_core::EntityId;
use std::collections::BTreeSet;

impl GameWorld {
    pub(super) fn restore_player_queues(
        &mut self,
        records: &[UnitRecord],
        failed_moves: &BTreeSet<EntityId>,
    ) {
        // A unit can retain pending player legs behind a formation or combat
        // job. Appending through issue_queued_move could instead start a new
        // leg immediately when it has no current movement route.
        for record in records {
            let id = record.unit.id;
            if record.queue.is_empty()
                || self.unit(id).is_none_or(|u| u.hp == 0)
                || self.embarked(id)
            {
                continue;
            }
            let occupied = self.formation_destinations(id).map_or_else(
                || usize::from(self.movement_order(id).is_some()),
                |destinations| destinations.len(),
            );
            let available = Self::move_queue_width().saturating_sub(occupied.max(1));
            if available == 0 {
                continue;
            }
            let queue = record.queue.iter().copied().take(available).collect();
            self.player_override(id);
            self.move_queues.insert(id, queue);
            // Only a refused scalar move needs this recovery: successful
            // attack, gather and other jobs must retain their current intent.
            // Generals retry their manager's current intent through decisions.
            if failed_moves.contains(&id) && !self.economy.generals.armies.contains_key(&id) {
                self.resume_queued_moves(&mut [id]);
            }
        }
    }
}
