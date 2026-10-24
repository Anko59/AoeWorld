//! AoE2 shepherding: a villager leads its sheep to the nearest food drop-off
//! (a Town Center or Mill) and slaughters it there, so the carcass is worked
//! with short drop-off trips.
use super::{AnimalKind, GameAnimal, HuntJob, HuntStage, distance, radius};
use crate::game_world::economy::placement::{distance_squared, perimeter, position};
use crate::{BuildingKind, GameUnit, GameWorld, UnitTask};
use aoe_core::{EntityId, PlayerId, WorldPosition};
use aoe_map::ResourceKind;

impl GameWorld {
    pub(super) fn shepherd_step(
        &mut self,
        id: EntityId,
        job: &mut HuntJob,
        u: GameUnit,
        a: GameAnimal,
    ) -> bool {
        let index = self.lookup[&id];
        let near = distance(u.position, a.position) <= radius(1);
        match job.stage {
            HuntStage::Approach => {
                if !near {
                    self.units[index].state.task = UnitTask::Moving;
                    return self.chase(id, job, a.position);
                }
                job.stage = HuntStage::Kill;
                job.repath_tick = self.tick.0;
                if let Some(to) = self.lead_destination(u.player, a.position)
                    && self.issue_task_move(id, to).is_ok()
                {
                    job.stage = HuntStage::Lead(to);
                    // No arrival yet: the stall deadline starts on arrival.
                    job.repath_tick = u64::MAX;
                    // The lead route has not moved yet. Keep its walk pose
                    // until the movement phase starts it, rather than showing
                    // a strike with the stale approach facing for one tick.
                    self.units[index].state.task = UnitTask::Moving;
                    self.follow(job.animal, None);
                }
                true
            }
            HuntStage::Lead(_) => {
                let arrived = self.movement_order(id).is_none();
                self.units[index].state.task =
                    if !arrived && !self.units[index].state.moving && u.task == UnitTask::Moving {
                        UnitTask::Moving
                    } else {
                        UnitTask::Shepherding
                    };
                if !self.units[index].state.moving
                    && self.units[index].state.task == UnitTask::Shepherding
                {
                    self.units[index].state.face_towards(a.position);
                }
                if arrived && job.repath_tick == u64::MAX {
                    job.repath_tick = self.tick.0.saturating_add(u64::from(self.config.tick_hz));
                }
                // At the drop-off the sheep is slaughtered once it has caught
                // up, or where it stalled behind an obstacle.
                if arrived && (near || (!a.moving && self.tick.0 >= job.repath_tick)) {
                    job.stage = HuntStage::Kill;
                    job.repath_tick = self.tick.0;
                    self.follow(job.animal, None);
                    return true;
                }
                self.follow(job.animal, (!near).then_some(u.position));
                true
            }
            HuntStage::Kill => true,
        }
    }

    /// The sheep walks toward its shepherd; `None` halts it.
    pub(super) fn follow(&mut self, animal: EntityId, to: Option<WorldPosition>) {
        let to = to.map(|to| self.config.snap_ground_position(to));
        if let Some(a) = self.wildlife.animals.get_mut(&animal)
            && a.state.kind == AnimalKind::Sheep
            && a.state.hp > 0
        {
            a.herd = to;
            if to.is_some() {
                self.wildlife.engaged.insert(animal);
            }
        }
    }

    /// The nearest open perimeter tile of the player's nearest food drop-off.
    fn lead_destination(&self, player: PlayerId, from: WorldPosition) -> Option<WorldPosition> {
        let tile = from.tile_floor();
        let building = self
            .dropoff_order(player, tile, ResourceKind::Food)
            .into_iter()
            .filter_map(|id| self.building(id))
            .find(|b| b.kind != BuildingKind::Dock)?;
        let mut tiles = perimeter(building.footprint);
        tiles.retain(|t| {
            self.unit_tile_passable(crate::UnitKind::Villager, *t, &|| false)
                .unwrap_or(false)
                && !self.economy.blocked_tiles.contains(t)
        });
        let to = tiles
            .into_iter()
            .min_by_key(|t| (distance_squared(tile, *t), t.y, t.x))?;
        position(to).ok()
    }
}

#[cfg(test)]
mod tests;
