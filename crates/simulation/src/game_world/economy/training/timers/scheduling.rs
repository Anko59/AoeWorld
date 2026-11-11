//! Running queues wake at completion; blocked queues subscribe to their inputs.
use super::*;

impl GameWorld {
    #[cfg(test)]
    fn training_exit_available(&self, b: &GameBuilding) -> bool {
        let Some(entry) = b.training_queue.front() else {
            return false;
        };
        super::super::super::placement::perimeter(b.footprint)
            .into_iter()
            .any(|tile| {
                self.unit_tile_passable(entry.kind, tile, &|| false)
                    .unwrap_or(false)
                    && !self.economy.blocked_tiles.contains(&tile)
                    && !self
                        .units()
                        .any(|u| !self.embarked(u.id) && u.position.tile_floor() == tile)
            })
    }

    pub(crate) fn sleep_training(&mut self, id: EntityId, exits: bool) {
        if self.economy.no_sleep {
            return;
        }
        let Some(b) = self.economy.buildings.get(&id) else {
            return;
        };
        let Some(entry) = b.training_queue.front() else {
            return;
        };
        let population = self.training_has_population(b.player);
        let remaining = entry.total_ticks.saturating_sub(entry.progress_ticks);
        let running = population && remaining > 0;
        let tiles = if exits {
            super::super::super::placement::perimeter(b.footprint)
        } else {
            Vec::new()
        };
        let clock = Clock {
            since: self.tick.0,
            until: running.then_some(self.tick.0.saturating_add(remaining)),
            running,
            #[cfg(test)]
            input: (entry.kind as u8, entry.progress_ticks, entry.total_ticks),
            population: Some(population),
        };
        self.economy.queues.training.insert(id, clock);
        if exits {
            self.economy.queues.unsubscribe(id);
            for &tile in &tiles {
                self.economy
                    .queues
                    .exits
                    .entry(tile)
                    .or_default()
                    .insert(id);
            }
            self.economy.queues.subscribed.insert(id, tiles);
        }
    }

    pub(crate) fn sleep_research(&mut self, id: EntityId) {
        if self.economy.no_sleep {
            return;
        }
        let Some(entry) = self.economy.buildings.get(&id).and_then(|b| b.research) else {
            return;
        };
        let clock = Clock {
            since: self.tick.0,
            until: Some(
                self.tick.0.saturating_add(
                    entry
                        .total_ticks
                        .saturating_sub(entry.progress_ticks)
                        .max(1),
                ),
            ),
            running: true,
            #[cfg(test)]
            input: (entry.tech as u8, entry.progress_ticks, entry.total_ticks),
            population: None,
        };
        self.economy.queues.research.insert(id, clock);
    }

    #[cfg(test)]
    pub(crate) fn assert_queue_dozers(&self, training: bool) {
        let clocks = if training {
            &self.economy.queues.training
        } else {
            &self.economy.queues.research
        };
        for (id, clock) in &clocks.clocks {
            let b = &self.economy.buildings[id];
            let input = if training {
                b.training_queue
                    .front()
                    .map(|e| (e.kind as u8, e.progress_ticks, e.total_ticks))
            } else {
                b.research
                    .map(|e| (e.tech as u8, e.progress_ticks, e.total_ticks))
            };
            assert_eq!(input, Some(clock.input), "missed wake: queue {id:?}");
            if let Some(population) = clock.population {
                assert_eq!(
                    population,
                    self.training_has_population(b.player),
                    "missed wake: queue population {id:?}"
                );
            }
            if training && self.economy.queues.subscribed.contains_key(id) {
                assert!(
                    !self.training_exit_available(b),
                    "missed wake: queue exit {id:?}"
                );
            }
        }
    }
}
