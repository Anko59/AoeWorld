//! Death, removal and building destruction.
use crate::{GameWorld, UnitTask};
use aoe_core::EntityId;

impl GameWorld {
    /// Cancel an owned actor's orders at its exact authoritative position.
    pub fn issue_stop(
        &mut self,
        player: aoe_core::PlayerId,
        id: EntityId,
    ) -> Result<(), crate::EconomyError> {
        if self.animal(id).is_some() {
            return self.stop_sheep(player, id);
        }
        let index = *self
            .lookup
            .get(&id)
            .ok_or(crate::EconomyError::UnknownEntity)?;
        let unit = self.units[index].state;
        if unit.hp == 0 {
            return Err(crate::EconomyError::InvalidTarget);
        }
        if unit.player != player {
            return Err(crate::EconomyError::NotOwner);
        }
        self.stop_movement(index);
        if let Ok(slot) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(slot);
        }
        self.stop_gathering(id);
        self.cancel_attack(id);
        self.economy.builders.remove(&id);
        self.units[index].last_movement_error = None;
        self.units[index].state.task = if unit.carried.is_some() {
            UnitTask::Carrying
        } else {
            UnitTask::Idle
        };
        Ok(())
    }

    /// Drops the unit's route and order without touching its task.
    pub(crate) fn stop_movement(&mut self, index: usize) {
        self.clear_planner(index);
        let unit = &mut self.units[index];
        unit.order = None;
        unit.route.clear();
        unit.state.moving = false;
        unit.state.planning = false;
    }

    /// Population is freed now; native death playback precedes thirty seconds of corpse decay.
    pub(crate) fn kill_unit(&mut self, index: usize) {
        self.stop_movement(index);
        let id = self.units[index].state.id;
        let player = self.units[index].state.player;
        let death_hunt = self
            .hunting_target(id)
            .and_then(|target| self.animal(target))
            .map(|a| a.kind);
        let state = &mut self.units[index].state;
        state.hp = 0;
        state.death_tick = Some(self.tick);
        state.death_hunt = death_hunt;
        state.task = UnitTask::Dying;
        state.attack_target = None;
        self.update_observer(id, None);
        self.combat.attackers.remove(&id);
        self.stop_gathering(id);
        self.cancel_hunt(id);
        self.economy.builders.remove(&id);
        if let Some(economy) = self.economy.players.get_mut(&player) {
            economy.population_used = economy.population_used.saturating_sub(1);
        }
        let die_millis = match self.units[index].state.kind {
            crate::UnitKind::Villager => 1500_u64,
            crate::UnitKind::Scout => 1900,
            crate::UnitKind::Militia => 1000,
        };
        let die_ticks = (u64::from(self.config.tick_hz) * die_millis).div_ceil(1000);
        let due = self.tick.0.saturating_add(
            die_ticks + u64::from(self.config.tick_hz) * u64::from(super::CORPSE_SECONDS),
        );
        self.combat.dying.insert((due, id));
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
        self.stop_gathering(id);
        self.economy.builders.remove(&id);
        self.lookup.remove(&id);
        self.update_observer(id, None);
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
