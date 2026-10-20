use super::{AnimalKind, AnimalTask, HuntJob, MAX_HUNTERS, distance, radius};
use crate::{EconomyError, GameUnit, GameWorld, UnitKind, UnitTask, unit_combat};
use aoe_core::{EntityId, PlayerId};

impl GameWorld {
    /// Validates the whole group before replacing any jobs. Reissuing the
    /// same hunt retains its cooldown, including while walking to the animal.
    pub fn issue_hunt(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        animal: EntityId,
    ) -> Result<(), EconomyError> {
        let a = self.huntable(player, animal)?;
        let mut group = ids.to_vec();
        group.sort_unstable();
        group.dedup();
        if group.is_empty() || group.len() != ids.len() || group.len() > crate::MAX_BUILD_GROUP {
            return Err(EconomyError::Capacity);
        }
        if self
            .wildlife
            .hunts
            .keys()
            .filter(|id| !group.contains(id))
            .count()
            + group.len()
            > MAX_HUNTERS
        {
            return Err(EconomyError::Capacity);
        }
        for id in &group {
            let u = self
                .unit(*id)
                .filter(|u| u.hp > 0)
                .ok_or(EconomyError::UnknownEntity)?;
            if u.player != player {
                return Err(EconomyError::NotOwner);
            }
            if u.kind != UnitKind::Villager {
                return Err(EconomyError::WrongUnitKind);
            }
        }
        for id in group {
            if self
                .wildlife
                .hunts
                .get(&id)
                .is_some_and(|job| job.animal == animal)
            {
                continue;
            }
            if self
                .economy
                .gatherers
                .get(&id)
                .is_some_and(|job| job.animal == Some(animal))
            {
                continue;
            }
            self.stop_gathering(id);
            self.cancel_attack(id);
            self.economy.builders.remove(&id);
            let index = self.lookup[&id];
            self.stop_movement(index);
            // A new hunt plans its own route; an earlier failure must not end it.
            self.units[index].last_movement_error = None;
            if self.units[index]
                .state
                .carried
                .is_some_and(|c| c.kind != aoe_map::ResourceKind::Food)
            {
                self.units[index].state.carried = None;
            }
            self.units[index].state.task = if a.kind == AnimalKind::Sheep {
                UnitTask::Shepherding
            } else {
                UnitTask::Hunting
            };
            self.wildlife.hunts.insert(
                id,
                HuntJob {
                    animal,
                    repath_tick: self.tick.0,
                },
            );
        }
        Ok(())
    }

    pub(super) fn advance_hunters(&mut self, changed: &mut Vec<GameUnit>) {
        let ids = self.wildlife.hunts.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(mut job) = self.wildlife.hunts.remove(&id) else {
                continue;
            };
            self.wildlife.stats.hunters += 1;
            let Some(before) = self.unit(id).filter(|u| u.hp > 0) else {
                continue;
            };
            let keep = self.hunter_step(id, &mut job);
            if keep {
                self.wildlife.hunts.insert(id, job);
            } else if !self.economy.gatherers.contains_key(&id) {
                let index = self.lookup[&id];
                self.stop_movement(index);
                self.units[index].state.task = if self.units[index].state.carried.is_some() {
                    UnitTask::Carrying
                } else {
                    UnitTask::Idle
                };
            }
            if let Some(after) = self.unit(id)
                && after != before
            {
                changed.push(after);
            }
        }
    }

    fn hunter_step(&mut self, id: EntityId, job: &mut HuntJob) -> bool {
        let Some(u) = self.unit(id) else {
            return false;
        };
        let Ok(a) = self.huntable(u.player, job.animal) else {
            return false;
        };
        if a.hp == 0 {
            if a.task != AnimalTask::Carcass {
                return true;
            }
            return self.begin_carcass_gather(id, job.animal).is_err();
        }
        let index = self.lookup[&id];
        if distance(u.position, a.position) > radius(1) {
            if self.movement_order(id).is_none() && self.movement_failure(id).is_some() {
                return false;
            }
            if self.tick.0 >= job.repath_tick {
                job.repath_tick = self.tick.0.saturating_add(self.repath_ticks());
                if self.issue_task_move(id, a.position).is_err() {
                    return false;
                }
            }
            return true;
        }
        self.stop_movement(index);
        self.units[index].state.facing = super::super::facing_for(
            i64::from(a.position.x) - i64::from(u.position.x),
            i64::from(a.position.y) - i64::from(u.position.y),
            u.facing,
        );
        self.units[index].state.task = if a.kind == AnimalKind::Sheep {
            UnitTask::Shepherding
        } else {
            UnitTask::Hunting
        };
        if self.tick.0 < self.units[index].next_hit_tick {
            return true;
        }
        let Some(target) = self.wildlife.animals.get_mut(&job.animal) else {
            return false;
        };
        target.state.hp = target.state.hp.saturating_sub(unit_combat(u.kind).attack);
        if a.kind == AnimalKind::Boar {
            target.state.target = Some(id);
            self.wildlife.engaged.insert(job.animal);
        }
        if target.state.hp == 0 {
            self.kill_animal(job.animal);
        }
        self.units[index].next_hit_tick = self
            .tick
            .0
            .saturating_add(u64::from(self.attack_period_ticks(&u)));
        true
    }
}
