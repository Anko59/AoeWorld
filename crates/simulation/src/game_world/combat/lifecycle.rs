//! Death, removal and building destruction.
use crate::{GameWorld, UnitTask};
use aoe_core::EntityId;

impl GameWorld {
    /// Drops the unit's route and order without touching its task.
    pub(crate) fn stop_movement(&mut self, index: usize) {
        self.clear_planner(index);
        let unit = &mut self.units[index];
        unit.order = None;
        unit.route.clear();
        unit.state.moving = false;
        unit.state.planning = false;
    }

    /// Population is freed now; the body stays for `DYING_SECONDS` at 0 HP.
    pub(crate) fn kill_unit(&mut self, index: usize) {
        self.stop_movement(index);
        let id = self.units[index].state.id;
        let player = self.units[index].state.player;
        let state = &mut self.units[index].state;
        state.hp = 0;
        state.task = UnitTask::Dying;
        state.attack_target = None;
        self.combat.attackers.remove(&id);
        self.economy.gatherers.remove(&id);
        self.economy.builders.remove(&id);
        if let Some(economy) = self.economy.players.get_mut(&player) {
            economy.population_used = economy.population_used.saturating_sub(1);
        }
        let due = self
            .tick
            .0
            .saturating_add(u64::from(self.config.tick_hz) * u64::from(super::DYING_SECONDS));
        self.combat.dying.push_back((due, id));
    }

    /// Removes a unit from every index. Its slot is filled by the last unit.
    pub(crate) fn remove_unit(&mut self, id: EntityId) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        self.clear_planner(index);
        let bucket = self.units[index].bucket;
        let slot = self.units[index].bucket_slot;
        if let Some(ids) = self.chunks.get_mut(&bucket) {
            ids.swap_remove(slot);
            let displaced = ids.get(slot).copied();
            let emptied = ids.is_empty();
            if let Some(displaced_index) = displaced.and_then(|id| self.lookup.get(&id).copied()) {
                self.units[displaced_index].bucket_slot = slot;
            }
            if emptied {
                self.chunks.remove(&bucket);
            }
        }
        if let Ok(position) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(position);
        }
        self.combat.attackers.remove(&id);
        self.economy.gatherers.remove(&id);
        self.economy.builders.remove(&id);
        self.lookup.remove(&id);
        self.units.swap_remove(index);
        if let Some(moved) = self.units.get(index) {
            self.lookup.insert(moved.state.id, index);
        }
    }

    /// A building at 0 HP disappears; its tiles become walkable and the
    /// population it supported goes away. Queued training is lost unrefunded.
    pub(crate) fn destroy_building(&mut self, id: EntityId) {
        self.remove_building(id);
    }
}
