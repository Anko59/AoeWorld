//! Group proposals share the immutable parallel batch and ordered index/vision
//! writes with ordinary movers. Body collision revalidation stays until PR 7.
use super::*;
use crate::UnitKind;
use crate::game_world::formation::{Motion, group_step};

impl GameWorld {
    fn group_tile_sealed(&self, kind: UnitKind, from: WorldPosition) -> bool {
        let tile = from.tile_floor();
        (-1..=1).all(|dy| {
            (-1..=1).all(|dx| {
                if dx == 0 && dy == 0 {
                    return true;
                }
                self.group_work.collision(1);
                let to = aoe_core::TileCoord::new(tile.x + dx, tile.y + dy);
                !self.unit_step_clear(kind, tile, to).unwrap_or(false)
                    || !self.building_step_clear(tile, to)
            })
        })
    }

    fn group_static_step_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
        certified: bool,
    ) -> bool {
        if certified {
            // Clearance proves tile bodies/buildings, but not elevation edges.
            return from.tile_floor() == to.tile_floor()
                || self
                    .unit_step_clear(kind, from.tile_floor(), to.tile_floor())
                    .unwrap_or(false);
        }
        self.body_step_clear(kind, from, to)
            && self
                .unit_step_clear(kind, from.tile_floor(), to.tile_floor())
                .unwrap_or(false)
            && self.building_step_clear(from.tile_floor(), to.tile_floor())
    }

    fn group_sweep_clear(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        maximum_radius: i32,
        padding: i32,
    ) -> bool {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("avoidance_query"));
        self.collision_sweep_bounded(
            index,
            from,
            to,
            false,
            false,
            Some((maximum_radius, padding)),
        )
    }

    pub(super) fn propose_group_step(&self, index: usize, tables: &Tables) -> Option<FastStep> {
        let mut motion = self.formation_motion(index)?;
        self.group_work.members(1);
        let stored = &self.units[index];
        let mut state = stored.state;
        let from = state.position;
        // An isolated tile has no exit, so do not creep toward its edge or
        // spend a detached route search. Keep the independent arrival wait.
        if !motion.static_clear && self.group_tile_sealed(state.kind, from) {
            motion.target = from;
            motion.center = from;
        }
        motion.neighbor_radius = tables.neighbor_radius[usize::from(state.kind.is_naval())];
        let mut position = if motion.done {
            from
        } else {
            group_step(from, motion.target, motion.budget)
        };
        let primary_static =
            self.group_static_step_clear(state.kind, from, position, motion.static_clear);
        if !primary_static {
            motion.obstructed = true;
            position = group_step(from, motion.center, motion.budget);
        }
        // Static terrain/buildings do not mutate during the movement phase.
        // The primary sweep already checked the body; only a fallback needs it.
        if !primary_static
            && !motion.static_clear
            && !self.body_step_clear(state.kind, from, position)
        {
            position = from;
        } else {
            // Overflow samples only 32 bodies. Even if an earlier ordered
            // step later removes the overflow, that snapshot proves nothing
            // about the omitted bodies: retain the ordinary ordered recheck.
            motion.certified = !self.collisions.has_overfull_cells()
                && self.group_sweep_clear(
                    index,
                    from,
                    position,
                    motion.neighbor_radius,
                    tables.group_step_bound,
                );
            if !motion.certified
                && !self.group_sweep_clear(index, from, position, motion.neighbor_radius, 0)
            {
                position = from;
            }
        }
        motion = motion.at(position);
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
        self.group_work.members(1);
        let from = self.units[index].state.position;
        if !motion.done
            && ((step.state.position == from && from != motion.target)
                || ((!motion.certified || self.collisions.has_overfull_cells())
                    && !self.group_sweep_clear(
                        index,
                        from,
                        step.state.position,
                        motion.neighbor_radius,
                        0,
                    )))
        {
            self.record_obstruction_towards(index, motion.target);
            let mut proposed = group_step(from, motion.target, motion.budget);
            let terrain_clear =
                self.group_static_step_clear(step.state.kind, from, proposed, motion.static_clear);

            if !terrain_clear {
                proposed = group_step(from, motion.center, motion.budget);
            }
            let position = self.avoid_unit_step(index, proposed);
            step.member = Some(
                Motion {
                    obstructed: motion.obstructed || !terrain_clear,
                    ..motion
                }
                .at(position),
            );
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
