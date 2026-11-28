//! Saves hold tick counts at `REFERENCE_HZ`, so a game resumes at the same
//! game time whatever rate the server steps at.
use super::{GameWorld, WorldSnapshot};

impl WorldSnapshot {
    /// This snapshot with every tick count and tick stamp moved from a world
    /// stepping at `from_hz` to one stepping at `to_hz`. A save stores them at
    /// `REFERENCE_HZ`, so it resumes at the same game time at any rate.
    pub fn retimed(mut self, from_hz: u32, to_hz: u32) -> Self {
        if from_hz == to_hz {
            return self;
        }
        let at = |tick: u64| {
            u64::try_from(u128::from(tick) * u128::from(to_hz) / u128::from(from_hz.max(1)))
                .unwrap_or(u64::MAX)
        };
        self.tick = at(self.tick);
        for player in &mut self.players {
            player.upkeep_slow_ticks = at(player.upkeep_slow_ticks);
        }
        for (_, tick) in &mut self.last_hostile_ticks {
            *tick = at(*tick);
        }
        self.relics.retime(at);
        for herd in &mut self.herd_recovery {
            herd.due = herd.due.map(at);
        }
        for record in &mut self.units {
            record.next_hit_tick = at(record.next_hit_tick);
            record.transform_until = record.transform_until.map(at);
            record.swing_started = record.swing_started.map(at);
            record.general_order_tick = record.general_order_tick.map(at);
            record.unit.death_tick = record.unit.death_tick.map(|t| aoe_core::Tick(at(t.0)));
            if let Some(hit) = &mut record.unit.last_hit {
                hit.tick = aoe_core::Tick(at(hit.tick.0));
            }
        }
        let remembered = self
            .building_memory
            .iter_mut()
            .flat_map(|record| &mut record.buildings);
        for record in self.buildings.iter_mut().chain(remembered) {
            if let Some(hit) = &mut record.last_hit {
                hit.tick = aoe_core::Tick(at(hit.tick.0));
            }
            // Construction and repair work is priced in the world's ticks.
            record.work_done = at(record.work_done);
            record.work_total = at(record.work_total).max(1);
            for entry in &mut record.training_queue {
                entry.progress_ticks = at(entry.progress_ticks);
                entry.total_ticks = at(entry.total_ticks).max(1);
            }
            if let Some(research) = &mut record.research {
                research.progress_ticks = at(research.progress_ticks);
                research.total_ticks = at(research.total_ticks).max(1);
            }
        }
        self
    }
}

impl GameWorld {
    /// Steps this world at `tick_hz`, speeds per second unchanged. Call it
    /// before the world advances or restores a save: tick stamps already held
    /// are not moved.
    pub fn set_tick_hz(&mut self, tick_hz: u32) {
        self.config = crate::movement_speed::at_tick_hz(self.config, tick_hz);
    }
}
