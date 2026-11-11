//! Writers materialize queue progress before changing its inputs.
use super::*;
use aoe_core::{PlayerId, TileRect};

impl GameWorld {
    pub(crate) fn wake_queues(&mut self, id: EntityId) {
        let timers = &mut self.economy.queues;
        let train_tick =
            timers.training.tick + u64::from(timers.cursor.is_some_and(|cursor| id < cursor));
        let train = timers.training.elapsed(id, train_tick);
        let research = timers.research.elapsed(id, timers.research.tick);
        timers.training.remove(id);
        timers.research.remove(id);
        timers.unsubscribe(id);
        if let Some(b) = self.economy.buildings.get_mut(&id) {
            if let Some(entry) = b.training_queue.front_mut() {
                entry.progress_ticks = entry
                    .progress_ticks
                    .saturating_add(train)
                    .min(entry.total_ticks);
                timers.training.awake.insert(id);
            }
            if let Some(entry) = &mut b.research {
                entry.progress_ticks = entry
                    .progress_ticks
                    .saturating_add(research)
                    .min(entry.total_ticks);
                timers.research.awake.insert(id);
            }
            if train > 0 || research > 0 {
                self.mark_building_changed(id);
            }
        }
    }

    pub(crate) fn forget_queues(&mut self, id: EntityId) {
        let timers = &mut self.economy.queues;
        if let Some(b) = self.economy.buildings.get(&id) {
            timers.unregister_training(id, b.player);
        }
        timers.training.remove(id);
        timers.research.remove(id);
        timers.training.awake.remove(&id);
        timers.research.awake.remove(&id);
        timers.unsubscribe(id);
    }

    pub(crate) fn wake_player_training(&mut self, player: PlayerId) {
        let ids = self
            .economy
            .queues
            .players
            .get(&player)
            .cloned()
            .unwrap_or_default();
        for id in ids {
            self.wake_queues(id);
        }
    }

    pub(crate) fn wake_queue_exits(&mut self, tile: TileCoord) {
        let ids = self
            .economy
            .queues
            .exits
            .get(&tile)
            .cloned()
            .unwrap_or_default();
        for id in ids {
            self.wake_queues(id);
        }
    }

    pub(crate) fn wake_queue_ground(&mut self, rect: TileRect) {
        let ids = self
            .economy
            .queues
            .exits
            .iter()
            .filter(|(tile, _)| rect.contains(**tile))
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect::<BTreeSet<_>>();
        for id in ids {
            self.wake_queues(id);
        }
    }

    pub(crate) fn begin_queues(&mut self, training: bool) -> Vec<EntityId> {
        let (clocks, active) = if training {
            (&self.economy.queues.training, &self.economy.active_training)
        } else {
            (&self.economy.queues.research, &self.economy.active_research)
        };
        let ids = if self.economy.no_sleep {
            active.iter().copied().collect::<Vec<_>>()
        } else {
            clocks
                .due
                .range(..=(self.tick.0, EntityId(u32::MAX)))
                .map(|(_, id)| *id)
                .collect()
        };
        for id in ids {
            self.wake_queues(id);
        }
        #[cfg(test)]
        self.assert_queue_dozers(training);
        let clocks = if training {
            &self.economy.queues.training
        } else {
            &self.economy.queues.research
        };
        clocks.awake.iter().copied().collect()
    }
}
