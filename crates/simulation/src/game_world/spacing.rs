//! Soft collision: keep passage open, but do not let crowds persist as one sprite.
use super::{GameWorld, GameWorldError};
use aoe_core::{ChunkCoord, EntityId, TileCoord, WorldPosition};

mod cells;
mod followers;
pub(crate) use followers::Followers;
mod destinations;
pub(crate) use destinations::Destinations;
mod idle;
pub(crate) use idle::IdleSpacing;

const SPACING: i64 = 768; // three quarters of a tile; crossing never hard-blocks
const PUSH: i64 = 32; // at most 45 subunits diagonally, once per four ticks

// Even two pushes cannot turn a nonadjacent-tile pair into an overlap.
const _: () = assert!(SPACING + 2 * PUSH < aoe_core::FIXED_SUBUNITS_PER_TILE as i64);

impl GameWorld {
    pub(crate) fn refresh_spacing_order(&mut self, id: EntityId) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let tile = self.units[index].state.position.tile_floor();
        if self.units[index].formation.is_some() {
            self.idle_spacing.remove(id, tile);
            self.followers.remove(id, tile);
        } else if self.units[index].order.is_some() {
            self.idle_spacing.remove(id, tile);
            self.followers.insert(id, tile);
        } else {
            self.followers.remove(id, tile);
            if self.units[index].state.hp > 0 && !self.embarked(id) {
                self.idle_spacing.insert(id, tile);
            }
        }
    }

    /// Pause only a follower in the same heading octant. Opposing/perpendicular
    /// traffic is permeable. The id breaks an exact tie, so one leader proceeds.
    /// Waiting discards this tick's whole speed credit instead of banking a dash.
    pub(crate) fn following_unit(&self, index: usize) -> bool {
        let unit = &self.units[index].state;
        let Some(order) = self.units[index].order.as_ref() else {
            return false;
        };
        if order.origin == order.waypoint {
            return false;
        }
        let mut direction = None;
        let is_leader = |id| {
            if id == unit.id {
                return false;
            }
            let Some(&other_index) = self.lookup.get(&id) else {
                return false;
            };
            let other = &self.units[other_index];
            if other.state.hp == 0 || !other.state.moving {
                return false;
            }
            let (dx, dy) = delta(other.state.position, unit.position);
            if dx * dx + dy * dy >= SPACING * SPACING {
                return false;
            }
            let direction = *direction.get_or_insert_with(|| heading(order.origin, order.waypoint));
            if other
                .order
                .as_ref()
                .is_none_or(|o| heading(o.origin, o.waypoint) != direction)
            {
                return false;
            }
            let ahead = dx * direction.0 + dy * direction.1;
            ahead > 0 || (ahead == 0 && id < unit.id)
        };
        let salt = self.tick.0.wrapping_add(u64::from(unit.id.0));
        self.followers
            .any_neighbour(unit.position.tile_floor(), salt, unit.id, is_leader)
    }

    pub(crate) fn advance_spacing(&mut self, changed: &mut Vec<EntityId>) {
        self.finish_spacing_orders(changed);
        // A pair closer than SPACING must occupy the same or adjacent tiles.
        // Rotate a bounded batch of due ids, then preserve storage order
        // within the batch (including swap-removal). An earlier push is at most
        // 32 subunits per axis: it cannot bring an initially nonadjacent tile
        // pair within SPACING during this pass. Neighbour sampling is unchanged.
        let mut indices = self
            .idle_spacing
            .take_candidates((self.tick.0 % 4) as usize)
            .into_iter()
            .filter_map(|id| self.lookup.get(&id).copied())
            .collect::<Vec<_>>();
        indices.sort_unstable();
        self.advance_spacing_indices(changed, indices);
    }

    pub(super) fn advance_spacing_indices(
        &mut self,
        changed: &mut Vec<EntityId>,
        indices: impl IntoIterator<Item = usize>,
    ) {
        #[cfg(test)]
        {
            self.idle_spacing.inspected_units = 0;
        }
        for index in indices {
            #[cfg(test)]
            {
                self.idle_spacing.inspected_units += 1;
            }
            let unit = self.units[index].state;
            if unit.hp == 0
                || self.units[index].order.is_some()
                || self.units[index].formation.is_some()
                || u64::from(unit.id.0) % 4 != self.tick.0 % 4
            {
                continue;
            }
            let (mut x, mut y) = (0, 0);
            for id in self.spacing_neighbours(
                unit.position,
                self.tick.0.wrapping_add(u64::from(unit.id.0)),
            ) {
                if id == unit.id {
                    continue;
                }
                let Some(&other_index) = self.lookup.get(&id) else {
                    continue;
                };
                let other = &self.units[other_index];
                // A passer-by does not dislodge an idle villager from its work.
                if other.state.hp == 0 || other.order.is_some() {
                    continue;
                }
                let (mut dx, mut dy) = delta(unit.position, other.state.position);
                if dx * dx + dy * dy >= SPACING * SPACING {
                    continue;
                }
                if dx == 0 && dy == 0 {
                    // Antisymmetric integer directions break perfect overlaps.
                    let low = unit.id.0.min(id.0);
                    let high = unit.id.0.max(id.0);
                    let pair = low.wrapping_mul(0x9e3779b9) ^ high.wrapping_mul(0x85ebca6b);
                    let direction = [(1, 0), (1, 1), (0, 1), (-1, 1)][pair as usize % 4];
                    let sign = if unit.id < id { 1 } else { -1 };
                    (dx, dy) = (direction.0 * sign, direction.1 * sign);
                }
                let scale = dx.abs().max(dy.abs());
                x += dx * PUSH / scale;
                y += dy * PUSH / scale;
            }
            let (x, y) = (x.clamp(-PUSH, PUSH) as i32, y.clamp(-PUSH, PUSH) as i32);
            if (x, y) == (0, 0) {
                continue;
            }
            // Try the combined push and then each axis: walls and resource
            // footprints constrain separation; they never become penetrable.
            for (dx, dy) in [(x, y), (x, 0), (0, y)] {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let at = WorldPosition::new(unit.position.x + dx, unit.position.y + dy);
                if !self.separation_clear(index, at) {
                    continue;
                }
                self.units[index].state.previous_position = unit.position;
                self.units[index].state.position = at;
                let after = self.units[index].state;
                self.track_unit_step(unit.id, unit.position.tile_floor(), &after);
                if self.units[index].bucket != ChunkCoord::from_position(at) {
                    self.move_bucket(index, ChunkCoord::from_position(at));
                }
                if unit.position.tile_floor() != at.tile_floor() {
                    self.update_unit_vision(unit.id);
                }
                changed.push(unit.id);
                break;
            }
        }
    }

    pub(super) fn finish_spacing_orders(&mut self, changed: &[EntityId]) {
        // Re-enrol finished movers before selecting the bounded idle batch.
        for &id in changed {
            if self.movement_order(id).is_none() {
                self.destinations.remove(id);
                self.refresh_spacing_order(id);
            }
        }
    }

    fn separation_clear(&self, index: usize, at: WorldPosition) -> bool {
        let unit = self.units[index].state;
        let tile = at.tile_floor();
        if !self.config.valid_ground_position(at)
            || !self.building_step_clear(unit.position.tile_floor(), tile)
            || !self
                .unit_step_clear(unit.kind, unit.position.tile_floor(), tile)
                .unwrap_or(false)
        {
            return false;
        }
        // A working villager must remain adjacent to its resource; packed
        // footprints can limit the achievable spacing without cancelling work.
        self.work_separation_clear(unit.id, tile)
    }
}
fn delta(a: WorldPosition, b: WorldPosition) -> (i64, i64) {
    (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}
fn heading(a: WorldPosition, b: WorldPosition) -> (i64, i64) {
    let (x, y) = delta(b, a);
    (x.signum(), y.signum())
}
