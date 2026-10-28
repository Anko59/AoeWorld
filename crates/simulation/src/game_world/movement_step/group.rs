//! Group proposals share the immutable parallel batch and ordered index/vision
//! writes with ordinary movers. Body collision revalidation stays until PR 7.
use super::*;
use crate::UnitKind;
use crate::game_world::formation::{Motion, group_step};

impl GameWorld {
    fn group_static_step_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        self.body_step_clear(kind, from, to)
            && self
                .unit_step_clear(kind, from.tile_floor(), to.tile_floor())
                .unwrap_or(false)
            && self.building_step_clear(from.tile_floor(), to.tile_floor())
    }

    pub(super) fn propose_group_step(&self, index: usize, tables: &Tables) -> Option<FastStep> {
        let mut motion = self.formation_motion(index)?;
        let stored = &self.units[index];
        let mut state = stored.state;
        let from = state.position;
        let mut position = if motion.done {
            from
        } else {
            group_step(from, motion.target, motion.budget)
        };
        if !self.group_static_step_clear(state.kind, from, position) {
            motion.obstructed = true;
            position = group_step(from, motion.center, motion.budget);
        }
        if !self.local_unit_step_clear(index, from, position) {
            position = from;
        }
        group_state(&mut state, motion, position);
        Some(FastStep {
            index,
            vision_stale: self.unit_vision_stale(&state, tables.economy(self, state.player)),
            bucket: ChunkCoord::from_position(position),
            state,
            order: stored.order,
            pops: 0,
            keeps_moving: false,
            following: false,
            member: Some(motion),
        })
    }

    pub(super) fn adjust_group_step(&mut self, step: &mut FastStep, motion: Motion) {
        let index = step.index;
        let from = self.units[index].state.position;
        if !motion.done
            && ((step.state.position == from && from != motion.target)
                || !self.local_unit_step_clear(index, from, step.state.position))
        {
            self.record_obstruction_towards(index, motion.target);
            let mut proposed = group_step(from, motion.target, motion.budget);
            let terrain_clear = self.group_static_step_clear(step.state.kind, from, proposed);
            step.member = Some(Motion {
                obstructed: motion.obstructed || !terrain_clear,
                ..motion
            });
            if !terrain_clear {
                proposed = group_step(from, motion.center, motion.budget);
            }
            let position = self.avoid_unit_step(index, proposed);
            step.state = self.units[index].state;
            group_state(&mut step.state, motion, position);
            step.bucket = ChunkCoord::from_position(position);
            step.vision_stale =
                self.unit_vision_stale(&step.state, self.player_economy(step.state.player));
        }
    }
}

fn group_state(state: &mut GameUnit, motion: Motion, position: WorldPosition) {
    let previous = state.position;
    state.previous_position = previous;
    state.position = position;
    // Queued waypoints are passing points. Final slots arrive independently,
    // including before the virtual anchor, so their members can fight at once.
    let arrived = motion.final_leg && (motion.done || position == motion.goal);
    state.moving = !arrived;
    state.task = if arrived {
        UnitTask::Idle
    } else {
        UnitTask::Moving
    };
    state.facing = if arrived {
        motion.facing
    } else {
        facing_for(
            i64::from(position.x - previous.x),
            i64::from(position.y - previous.y),
            motion.facing,
        )
    };
}
