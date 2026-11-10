//! Exact shortcuts; required bookkeeping stays ordered on the tick thread.
use super::*;
use crate::UnitKind;
use aoe_core::TileCoord;

impl GameWorld {
    pub(crate) fn census_apply(&self, index: usize, to: WorldPosition) {
        let Some(probe) = &self.planning_probe else {
            return;
        };
        probe.movement_count("applied", 1);
        let unit = self.units[index].state;
        let from = unit.position;
        let (cell_clear, min, max) = self.sweep_cells_proof(index, from, to, 0);
        let body_free = cell_clear
            || !self
                .collisions
                .census_any(min, max, unit.kind.is_naval(), |id, at| {
                    if id == unit.id {
                        return false;
                    }
                    let other = self.units[self.collisions.unit_index(id)].state;
                    other.hp > 0
                        && swept_body_intersects(
                            from,
                            to,
                            at,
                            i64::from(unit.kind.collision_radius() + other.kind.collision_radius()),
                        )
                });
        let static_clear = body_free
            && self.body_step_clear(unit.kind, from, to)
            && self
                .unit_position_step_clear(unit.kind, from, to)
                .unwrap_or(false)
            && self.building_step_clear(from.tile_floor(), to.tile_floor());
        if static_clear {
            probe.movement_count("applied_no_body", 1);
        }
        if cell_clear && static_clear {
            probe.movement_count("applied_clear", 1);
        }
        if from == to {
            probe.movement_count("applied_stationary", 1);
        }
    }

    /// Conservative live-cell proof: includes every centre in the swept AABB,
    /// expanded by both hulls. Static footprints are proved separately.
    fn sweep_cells_proof(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        padding: i32,
    ) -> (bool, TileCoord, TileCoord) {
        let unit = &self.units[index].state;
        let radius = unit.kind.collision_radius()
            + self.collisions.maximum_radii()[usize::from(unit.kind.is_naval())]
            + padding;
        let min = WorldPosition::new(
            from.x.min(to.x).saturating_sub(radius),
            from.y.min(to.y).saturating_sub(radius),
        )
        .tile_floor();
        let max = WorldPosition::new(
            from.x.max(to.x).saturating_add(radius),
            from.y.max(to.y).saturating_add(radius),
        )
        .tile_floor();
        (
            self.collisions
                .cells_empty_except(min, max, unit.kind.is_naval(), unit.id),
            min,
            max,
        )
    }

    /// No physical state changes, route consumption or side effects. An order
    /// (usually fractional credit while waiting) may still need committing.
    pub(super) fn apply_is_stationary(&self, step: &FastStep) -> bool {
        self.units.get(step.index).is_some_and(|stored| {
            step.member.is_none()
                && stored.state.position == step.state.position
                && stored.state == step.state
                && stored.obstruction.is_none()
                && step.pops == 0
                && !step.vision_stale
                && stored.bucket == step.bucket
        })
    }

    pub(crate) fn apply_step(
        &mut self,
        step: FastStep,
        still_moving: &mut Vec<EntityId>,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        if self.apply_is_stationary(&step) && !self.arrival_slot_occupied(step.index) {
            let no_op = self.units[step.index].order == step.order;
            self.assert_stationary_oracle(&step);
            self.census_apply(step.index, step.state.position);
            if self.units[step.index].order != step.order {
                self.units[step.index].order = step.order;
            }
            if step.keeps_moving {
                still_moving.push(step.state.id);
            }
            if let Some(probe) = &self.planning_probe {
                probe.movement_count("stationary_apply_fast", 1);
                if no_op {
                    probe.movement_count("noop_apply_fast", 1);
                }
            }
            return true;
        }
        self.apply_step_full(step, still_moving, changed)
    }

    pub(crate) fn settle_movement_if_needed(&mut self, index: usize) -> bool {
        if self.units[index].obstruction.is_none() {
            self.assert_settling_oracle(index);
            if let Some(probe) = &self.planning_probe {
                probe.movement_count("settling_skipped", 1);
            }
            return false;
        }
        self.settle_obstructed_unit(index)
    }
}

impl Tables {
    pub(super) fn cache_group_cells(
        &mut self,
        world: &GameWorld,
        indices: impl IntoIterator<Item = usize>,
    ) {
        self.sealed_cells.clear();
        for index in indices {
            let unit = &world.units[index];
            if unit
                .formation
                .and_then(|id| world.formations.get(&id))
                .is_some_and(|group| group.static_covers(unit.state.position, unit.state.kind))
            {
                continue;
            }
            let tile = unit.state.position.tile_floor();
            self.sealed_cells
                .entry((tile, unit.state.kind as u8))
                .or_insert_with(|| world.uncached_group_cell_sealed(unit.state.kind, tile));
        }
        if let Some(probe) = &world.planning_probe {
            probe.movement_count("sealed_cells_cached", self.sealed_cells.len());
        }
    }

    pub(super) fn group_cell_sealed(
        &self,
        world: &GameWorld,
        kind: UnitKind,
        tile: TileCoord,
    ) -> bool {
        if let Some(&sealed) = self.sealed_cells.get(&(tile, kind as u8)) {
            self.assert_cell_oracle(world, kind, tile, sealed);
            if let Some(probe) = &world.planning_probe {
                probe.movement_count("sealed_cell_cache_hits", 1);
            }
            sealed
        } else {
            world.uncached_group_cell_sealed(kind, tile)
        }
    }
}

/// Pure capsule census, independent of contact policy and spawn escape.
fn swept_body_intersects(
    from: WorldPosition,
    to: WorldPosition,
    at: WorldPosition,
    radius: i64,
) -> bool {
    let (dx, dy) = (
        i64::from(to.x) - i64::from(from.x),
        i64::from(to.y) - i64::from(from.y),
    );
    let (x, y) = (
        i64::from(at.x) - i64::from(from.x),
        i64::from(at.y) - i64::from(from.y),
    );
    let dot = x * dx + y * dy;
    let length = dx * dx + dy * dy;
    if dot <= 0 {
        x * x + y * y < radius * radius
    } else if dot >= length {
        let (x, y) = (
            i64::from(at.x) - i64::from(to.x),
            i64::from(at.y) - i64::from(to.y),
        );
        x * x + y * y < radius * radius
    } else {
        let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
        cross * cross < i128::from(radius * radius) * i128::from(length)
    }
}

#[cfg(not(test))]
impl GameWorld {
    #[inline(always)]
    fn assert_stationary_oracle(&mut self, _: &FastStep) {}
    #[inline(always)]
    fn assert_settling_oracle(&mut self, _: usize) {}
}
#[cfg(not(test))]
impl Tables {
    #[inline(always)]
    fn assert_cell_oracle(&self, _: &GameWorld, _: UnitKind, _: TileCoord, _: bool) {}
}

#[cfg(test)]
impl GameWorld {
    pub(super) fn sweep_cells_clear(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        padding: i32,
    ) -> bool {
        self.sweep_cells_proof(index, from, to, padding).0
    }
    fn assert_stationary_oracle(&mut self, step: &FastStep) {
        crate::game_world::route_scheduler::GroupWork::unmeasured(|| {
            tests::assert_stationary_apply(self, step.clone())
        });
    }
    fn assert_settling_oracle(&mut self, index: usize) {
        crate::game_world::route_scheduler::GroupWork::unmeasured(|| {
            tests::assert_settling_skip(self, index)
        });
    }
}
#[cfg(test)]
impl Tables {
    fn assert_cell_oracle(&self, world: &GameWorld, kind: UnitKind, tile: TileCoord, sealed: bool) {
        let full = crate::game_world::route_scheduler::GroupWork::unmeasured(|| {
            world.uncached_group_cell_sealed(kind, tile)
        });
        assert_eq!(sealed, full);
    }
}

#[cfg(test)]
mod tests;
