use super::{AnimalTask, cell};
use crate::GameWorld;
use aoe_core::EntityId;

impl GameWorld {
    pub(super) fn kill_animal(&mut self, id: EntityId) {
        let Some(a) = self.wildlife.animals.get_mut(&id) else {
            return;
        };
        a.state.hp = 0;
        a.state.task = AnimalTask::Dead;
        a.state.target = None;
        let key = cell(a.state.position);
        self.wildlife.engaged.remove(&id);
        self.unindex_animal(id, key);
        self.wildlife.due.insert((
            self.tick.0.saturating_add(u64::from(self.config.tick_hz)),
            id,
        ));
    }

    pub(super) fn advance_carcasses(&mut self) {
        while let Some(&(due, id)) = self.wildlife.due.first() {
            if due > self.tick.0 {
                break;
            }
            self.wildlife.due.remove(&(due, id));
            self.wildlife.stats.carcasses += 1;
            let attended = self
                .economy
                .gatherers
                .values()
                .any(|t| t.animal == Some(id))
                || self.wildlife.hunts.values().any(|t| t.animal == id);
            let Some(a) = self.wildlife.animals.get_mut(&id) else {
                continue;
            };
            a.state.task = AnimalTask::Carcass;
            if !attended {
                a.decay_carry += u64::from(a.state.kind.rules().decay_hundredths);
                let lost = (a.decay_carry / 100).min(u64::from(a.state.food_remaining)) as u16;
                a.decay_carry %= 100;
                a.state.food_remaining -= lost;
            }
            if a.state.food_remaining > 0 {
                self.wildlife
                    .due
                    .insert((due.saturating_add(u64::from(self.config.tick_hz)), id));
            }
        }
    }

    pub(crate) fn deplete_carcass(&mut self, id: EntityId) -> Option<u16> {
        let a = self.wildlife.animals.get_mut(&id)?;
        if a.state.hp > 0 || a.state.task != AnimalTask::Carcass || a.state.food_remaining == 0 {
            return None;
        }
        a.state.food_remaining -= 1;
        Some(a.state.food_remaining)
    }
}
