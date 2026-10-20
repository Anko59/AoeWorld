use super::{AnimalTask, cell};
use crate::GameWorld;
use aoe_core::EntityId;

impl GameWorld {
    pub(crate) fn strike_animal(&mut self, id: EntityId, attacker: EntityId, damage: u16) -> bool {
        let Some(a) = self.wildlife.animals.get_mut(&id) else {
            return true;
        };
        a.state.hp = a.state.hp.saturating_sub(damage.max(1));
        if a.state.hp == 0 {
            self.kill_animal(id);
            true
        } else {
            if matches!(
                a.state.kind,
                super::AnimalKind::Wolf | super::AnimalKind::Boar
            ) {
                a.state.target = Some(attacker);
                a.state.task = AnimalTask::Attacking;
                self.wildlife.engaged.insert(id);
            }
            false
        }
    }
    pub(super) fn kill_animal(&mut self, id: EntityId) {
        let Some(a) = self.wildlife.animals.get_mut(&id) else {
            return;
        };
        a.state.hp = 0;
        a.state.moving = false;
        a.state.death_tick = Some(self.tick);
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
            let expiry = a.state.death_tick.map_or(due, |tick| {
                tick.0.saturating_add(
                    u64::from(self.config.tick_hz)
                        * u64::from(crate::CORPSE_SECONDS + crate::DYING_SECONDS),
                )
            });
            if a.state.food_remaining > 0 || due < expiry {
                self.wildlife
                    .due
                    .insert((due.saturating_add(u64::from(self.config.tick_hz)), id));
            } else {
                self.wildlife.animals.remove(&id);
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
