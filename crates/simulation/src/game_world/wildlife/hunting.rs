use super::{AnimalKind, AnimalTask, HuntJob, HuntStage, distance, radius};
use crate::{EconomyError, GameWorld, UnitKind, UnitTask, unit_combat};
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
            return Err(EconomyError::OrderTooLarge);
        }
        let rehunting = group
            .iter()
            .filter(|id| self.wildlife.hunts.contains_key(id))
            .count();
        if self.wildlife.hunts.len() - rehunting + group.len() > self.economy.limits.hunters {
            return Err(EconomyError::Capacity);
        }
        for id in &group {
            let u = self
                .unit(*id)
                .filter(|u| u.hp > 0 && !self.embarked(u.id))
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
            // A villager walks to a living sheep; herding starts once it leads.
            self.units[index].state.task = if a.kind != AnimalKind::Sheep {
                UnitTask::Hunting
            } else if a.hp > 0 {
                UnitTask::Moving
            } else {
                UnitTask::Shepherding
            };
            self.wildlife.hunts.insert(
                id,
                HuntJob {
                    animal,
                    repath_tick: self.tick.0,
                    stage: if a.kind == AnimalKind::Sheep {
                        HuntStage::Approach
                    } else {
                        HuntStage::Kill
                    },
                },
            );
        }
        Ok(())
    }

    pub(super) fn advance_hunters(&mut self, changed: &mut Vec<EntityId>) {
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
                changed.push(after.id);
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
        if a.kind == AnimalKind::Sheep && job.stage != HuntStage::Kill {
            let keep = self.shepherd_step(id, job, u, a);
            // A sheep that needs no leading is struck on the tick its shepherd decides.
            if !keep || job.stage != HuntStage::Kill {
                return keep;
            }
        }
        let index = self.lookup[&id];
        let reach = if a.kind == AnimalKind::Sheep { 1 } else { 4 };
        if distance(u.position, a.position) > radius(reach) {
            // Walking into reach is a walk, never the attack pose.
            self.units[index].state.task = if a.kind == AnimalKind::Sheep {
                UnitTask::Moving
            } else {
                UnitTask::Hunting
            };
            return self.chase(id, job, a.position);
        }
        if a.kind == AnimalKind::Sheep {
            self.follow(job.animal, None);
        }
        self.stop_movement(index);
        self.units[index].state.face_towards(a.position);
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

    /// Walks toward `to`, repathing at the shared interval; a failed route ends the job.
    pub(super) fn chase(
        &mut self,
        id: EntityId,
        job: &mut HuntJob,
        to: aoe_core::WorldPosition,
    ) -> bool {
        if self.movement_order(id).is_none() && self.movement_failure(id).is_some() {
            return false;
        }
        // A route still being searched is not restarted (see `waiting_for_route`).
        let waiting = self
            .lookup
            .get(&id)
            .is_some_and(|&index| self.waiting_for_route(index));
        if self.tick.0 >= job.repath_tick && !waiting {
            job.repath_tick = self.tick.0.saturating_add(self.repath_ticks());
            let to = self.config.snap_ground_position(to);
            // Keep the retained detour and its obstruction allowance while
            // prey stays in the same tile. Restarting each chase interval can
            // turn the hunter back before it ever clears a crowded doorway.
            if self
                .movement_order(id)
                .is_none_or(|order| order.destination != to)
                && self.issue_task_move(id, to).is_err()
            {
                return false;
            }
        }
        true
    }
}
