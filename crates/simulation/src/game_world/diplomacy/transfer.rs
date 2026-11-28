use super::*;
use crate::{GameUnit, UnitKind, UnitTask};

impl GameWorld {
    pub(super) fn transfer_conquered_assets(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        changed: &mut Vec<EntityId>,
    ) {
        let mut units: Vec<_> = self
            .units()
            .filter(|u| u.player == from && u.hp > 0)
            .map(|u| u.id)
            .collect();
        units.sort_unstable();
        for id in units {
            let index = self.lookup[&id];
            let mut unit = self.units[index].state;
            let transport = self.economy.embarked.get(&id).copied();
            let relic = self.detach_conquered_relic(id);
            self.stop_unit_orders(index);
            self.cancel_garrison_order(id);
            self.cancel_trade(id);
            self.cancel_hunt(id);
            self.move_queues.remove(&id);
            if unit.kind == UnitKind::Steward {
                self.forget_steward(id);
            }
            self.remove_unit(id);
            reset_conquered_unit(&mut unit, to);
            self.insert_map_unit(unit, 0);
            self.attach_conquered_relic(id, relic);
            if let Some(transport) = transport {
                self.restore_embarked(id, transport);
            }
            self.change_unit_census(to, None, Some(unit.kind), 1);
            match unit.kind {
                UnitKind::General => self.register_general(id),
                UnitKind::Steward => self.register_steward(id),
                _ => {}
            }
            changed.push(id);
        }

        self.transfer_conquered_buildings(from, to);
        let mut sheltered = Vec::new();
        for (_, occupants) in self.economy.ejections.values_mut() {
            for occupant in occupants
                .iter_mut()
                .filter(|occupant| occupant.unit.player == from)
            {
                sheltered.push(occupant.unit.kind);
                reset_conquered_unit(&mut occupant.unit, to);
                occupant.intent = Default::default();
                occupant.steward = None;
                occupant.vassal = None;
            }
        }
        for kind in sheltered {
            self.change_unit_census(from, Some(kind), None, 1);
            self.change_unit_census(to, None, Some(kind), 1);
        }
        self.transfer_conquered_herds(from, to);
        let Some(loser) = self.economy.players.get_mut(&from) else {
            return;
        };
        let mut stockpile = std::mem::take(&mut loser.stockpile);
        let inhabitants = std::mem::take(&mut loser.population_used);
        let Some(winner) = self.economy.players.get_mut(&to) else {
            return;
        };
        winner.population_used = winner.population_used.saturating_add(inhabitants);
        for kind in [
            ResourceKind::Food,
            ResourceKind::Wood,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ] {
            let balance = resource_amount(&mut winner.stockpile, kind);
            *balance = balance.saturating_add(*resource_amount(&mut stockpile, kind));
        }
        self.refresh_population_cap(from);
        self.refresh_population_cap(to);
    }
}

pub(crate) fn reset_conquered_unit(unit: &mut GameUnit, to: PlayerId) {
    unit.player = to;
    unit.previous_position = unit.position;
    unit.moving = false;
    unit.planning = false;
    unit.auto_scout = false;
    unit.sergeant = false;
    unit.attack_target = None;
    unit.last_hit = None;
    unit.task = if unit.carried.is_some() {
        UnitTask::Carrying
    } else {
        UnitTask::Idle
    };
}
