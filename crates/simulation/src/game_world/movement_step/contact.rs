//! Body clearance and bounded contact policy for land and water.
use crate::{GameUnit, GameWorld, UnitKind, UnitTask};
use aoe_core::{TileCoord, WorldPosition};

impl UnitKind {
    /// Shore clearance is separate from a ship's half-tile hull.
    pub const fn terrain_radius(self) -> i32 {
        if self.is_naval() {
            256
        } else {
            self.collision_radius()
        }
    }
    /// Equal collision axes in the complete classic DAT table, rounded up to
    /// fixed subunits. See docs/movement/aoe2-model.md for source and confidence.
    pub const fn collision_radius(self) -> i32 {
        const RADIUS: [i16; 256] = {
            let mut table = [205; 256];
            table[UnitKind::Scout as usize] = 256;
            table[UnitKind::Knight as usize] = 256;
            table[UnitKind::LightCavalry as usize] = 256;
            table[UnitKind::Vassal as usize] = 308;
            table[UnitKind::General as usize] = 410;
            table[UnitKind::BatteringRam as usize] = 461;
            table[UnitKind::FishingShip as usize] = 512;
            table[UnitKind::Galley as usize] = 512;
            table[UnitKind::FireShip as usize] = 512;
            table[UnitKind::DemolitionShip as usize] = 512;
            table[UnitKind::TransportShip as usize] = 512;
            table[UnitKind::WarGalley as usize] = 512;
            table[UnitKind::Mangonel as usize] = 512;
            table
        };
        // One load: this runs for every body tested in a collision scan.
        RADIUS[self as usize] as i32
    }
}

impl GameWorld {
    #[inline(always)]
    fn stationary_work_distance(&self, unit: &GameUnit, other: &GameUnit) -> Option<i64> {
        if unit.kind.is_naval()
            || unit.player != other.player
            || other.moving
            || !(other.working() || self.economy.gatherers.contains_key(&other.id))
        {
            return None;
        }
        let sum = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
        Some((sum * 75 + 99) / 100)
    }

    #[inline(always)]
    pub(super) fn movement_contact_distance(
        &self,
        unit: &GameUnit,
        other: &GameUnit,
        soft: bool,
        narrow: bool,
        work_squeeze: bool,
    ) -> i64 {
        let sum = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
        if let Some(r) = self.naval_worker_distance(unit, other) {
            return r;
        }
        if narrow && other.moving {
            return sum / 2;
        }
        if (soft || work_squeeze)
            && let Some(r) = self.stationary_work_distance(unit, other)
        {
            return r;
        }
        if soft {
            super::collision::hard_distance(unit, other)
        } else {
            sum
        }
    }

    /// A preference is not a full-body veto once actual, legal work contact
    /// was reached. Working friends retain their graded hard distance.
    pub(crate) fn work_contact_clear(&self, index: usize) -> bool {
        self.work_point_clear(index, self.units[index].state.position)
    }

    pub(crate) fn work_point_clear(&self, index: usize, at: WorldPosition) -> bool {
        self.work_point_clear_for::<false>(index, at)
    }

    /// A queue may wait for moving or idle friendly traffic, but cannot take
    /// a settled worker's hard contact circle or a stationary enemy's body.
    pub(crate) fn work_queue_point_clear(&self, index: usize, at: WorldPosition) -> bool {
        self.work_point_clear_for::<true>(index, at)
    }

    fn work_point_clear_for<const QUEUE: bool>(&self, index: usize, at: WorldPosition) -> bool {
        let unit = self.units[index].state;
        !self.collisions.blocked(
            (at, unit.kind.collision_radius() + 512),
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = self.units[self.collisions.unit_index(id)].state;
                if other.hp == 0
                    || (QUEUE
                        && (other.moving
                            || (other.player == unit.player
                                && !other.working()
                                && !self.economy.gatherers.contains_key(&other.id))))
                {
                    return false;
                }
                let r = self
                    .naval_worker_distance(&unit, &other)
                    .or_else(|| self.stationary_work_distance(&unit, &other))
                    .unwrap_or_else(|| {
                        i64::from(unit.kind.collision_radius() + other.kind.collision_radius())
                    });
                let x = i64::from(at.x - other.position.x);
                let y = i64::from(at.y - other.position.y);
                x * x + y * y < r * r
            },
            |_| true,
        )
    }

    /// Bounded local demand: at most sixteen cells and an early exit once
    /// enough friendly centres fill a normal contact ring.
    pub(crate) fn friendly_contact_pressure(&self, index: usize, capacity: usize) -> bool {
        self.friendly_contact_pressure_at(index, self.units[index].state.position, capacity)
    }

    pub(crate) fn friendly_contact_pressure_at(
        &self,
        index: usize,
        at: WorldPosition,
        capacity: usize,
    ) -> bool {
        let unit = self.units[index].state;
        if !self
            .collisions
            .contact_may_be_crowded(at, capacity, &self.group_work)
        {
            return false;
        }
        let mut count = 1;
        self.collisions.blocked(
            (at, 1536),
            false,
            self.tick.0,
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = self.units[self.collisions.unit_index(id)].state;
                let x = i64::from(other.position.x - at.x);
                let y = i64::from(other.position.y - at.y);
                if other.hp > 0 && other.player == unit.player && x * x + y * y <= 1536 * 1536 {
                    count += 1;
                }
                count >= capacity
            },
            |_| true,
        )
    }

    #[inline(always)]
    pub(crate) fn naval_worker_distance(&self, unit: &GameUnit, other: &GameUnit) -> Option<i64> {
        (unit.kind.is_naval()
            && other.kind == UnitKind::FishingShip
            && unit.player == other.player
            && self.economy.gatherers.contains_key(&other.id))
        .then(|| i64::from(unit.kind.collision_radius() + other.kind.collision_radius()) / 2)
    }

    /// Only a real one-tile water lane permits emergency hull compression.
    /// Idle enemy hulls stay hard, as do moving ships in wider channels.
    pub(super) fn narrow_water_lane(&self, at: WorldPosition) -> bool {
        let here = at.tile_floor();
        let bank = |dx, dy| {
            let tile = TileCoord::new(here.x + dx, here.y + dy);
            self.economy.blocked_tiles.contains(&tile)
                || !self
                    .unit_tile_passable(UnitKind::FishingShip, tile, &|| false)
                    .unwrap_or(false)
        };
        (bank(0, -1) && bank(0, 1)) || (bank(-1, 0) && bank(1, 0))
    }

    /// Bear right until the hulls can pass. Trim a lateral step to the last
    /// statically legal subunit; fixed-angle whole steps miss a narrow rail.
    pub(super) fn water_head_on_step(
        &self,
        index: usize,
        proposed: WorldPosition,
    ) -> Option<WorldPosition> {
        let unit = self.units[index].state;
        let from = unit.position;
        if !unit.kind.is_naval() || unit.task != UnitTask::Moving || !self.narrow_water_lane(from) {
            return None;
        }
        let (dx, dy) = (
            i64::from(proposed.x - from.x),
            i64::from(proposed.y - from.y),
        );
        let budget = crate::game_path::segment_length(dx, dy) as i64;
        if budget == 0 {
            return None;
        }
        let heading = if dx.abs() >= dy.abs() * 2 {
            (dx.signum(), 0)
        } else if dy.abs() >= dx.abs() * 2 {
            (0, dy.signum())
        } else {
            (dx, dy)
        };
        let length = crate::game_path::segment_length(heading.0, heading.1) as i64;
        let opposed = self.collisions.blocked(
            (from, 1024 + 2 * budget as i32),
            true,
            self.tick.0,
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = &self.units[self.collisions.unit_index(id)];
                let Some(order) = other.order else {
                    return false;
                };
                let (ox, oy) = (
                    i64::from(other.state.position.x - from.x),
                    i64::from(other.state.position.y - from.y),
                );
                let dot = ox * heading.0 + oy * heading.1;
                other.state.hp > 0
                    && other.state.moving
                    && dot > 0
                    && dot <= (1024 + 2 * budget) * length
                    && (ox * heading.1 - oy * heading.0).abs() < 1024 * length
                    && i64::from(order.waypoint.x - other.state.position.x) * heading.0
                        + i64::from(order.waypoint.y - other.state.position.y) * heading.1
                        < 0
            },
            |_| true,
        );
        if !opposed {
            return None;
        }
        let point = |x: i64, y: i64, distance: i64| {
            WorldPosition::new(
                from.x + (x * distance / length) as i32,
                from.y + (y * distance / length) as i32,
            )
        };
        let static_clear = |to| {
            self.body_step_clear(unit.kind, from, to)
                && self
                    .unit_step_clear(unit.kind, from.tile_floor(), to.tile_floor())
                    .unwrap_or(false)
                && self.building_step_clear(from.tile_floor(), to.tile_floor())
        };
        let (mut low, mut high) = (0, budget);
        while low < high {
            let mid = (low + high + 1) / 2;
            if static_clear(point(-heading.1, heading.0, mid)) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let lateral = point(-heading.1, heading.0, low);
        if lateral != from && self.local_unit_step_clear(index, from, lateral) {
            return Some(lateral);
        }
        let forward = point(heading.0, heading.1, budget);
        (static_clear(forward) && self.local_unit_step_clear(index, from, forward))
            .then_some(forward)
    }
}

#[cfg(test)]
mod tests;
