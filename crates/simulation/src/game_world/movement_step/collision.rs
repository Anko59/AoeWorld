//! Fine collision cells complement the wildlife index. Normal cells are exact;
//! overfull spawn cells admit outward escape, but never new arrivals.
use crate::{GameWorld, UnitKind, UnitTask};
use aoe_core::{TileCoord, WorldPosition};

#[path = "collision/bridge.rs"]
mod bridge;
mod index;
pub(crate) use index::{CellHasher, CollisionIndex, key};

/// Hard shares of the radii sum: friendly traffic 50%, workers 75%, others 100%.
/// Ships keep whole hulls outside narrow-water and fishing contacts (contact.rs).
const HARD_FRIEND_MOVING: i64 = 50;
const HARD_FRIEND_IDLE: i64 = 50;
const HARD_FRIEND_WORKING: i64 = 75;
const HARD_OTHER: i64 = 100;

impl crate::GameUnit {
    /// A body working in place: friendly traffic keeps most of its distance.
    #[inline(always)]
    pub(crate) fn working(&self) -> bool {
        !self.moving
            // A pursuit waiting for a route still occupies a combat position.
            // Its truthful Moving task must not grant friendly passing rights.
            && (self.attack_target.is_some() || matches!(
                self.task,
                UnitTask::Gathering
                    | UnitTask::Building
                    | UnitTask::Farming
                    | UnitTask::Fishing
                    | UnitTask::Hunting
                    | UnitTask::Shepherding
                    | UnitTask::Attacking
            ))
    }
}

/// The centre distance below which `unit` may never enter `other`.
#[inline(always)]
pub(crate) fn hard_distance(unit: &crate::GameUnit, other: &crate::GameUnit) -> i64 {
    let sum = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
    let percent = if unit.kind.is_naval() {
        100
    } else if unit.player != other.player {
        HARD_OTHER
    } else if other.moving {
        HARD_FRIEND_MOVING
    } else if other.working() {
        HARD_FRIEND_WORKING
    } else {
        HARD_FRIEND_IDLE
    };
    (sum * percent + 99) / 100
}

impl GameWorld {
    pub(super) fn collision_occupied(
        &self,
        index: usize,
        at: WorldPosition,
        stationary: bool,
    ) -> bool {
        let unit = &self.units[index].state;
        self.collisions.blocked(
            (at, unit.kind.collision_radius() + 512),
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = &self.units[self.collisions.unit_index(id)].state;
                let r = self.naval_worker_distance(unit, other).unwrap_or_else(|| {
                    i64::from(unit.kind.collision_radius() + other.kind.collision_radius())
                });
                let x = i64::from(at.x) - i64::from(other.position.x);
                let y = i64::from(at.y) - i64::from(other.position.y);
                other.hp > 0 && (!stationary || !other.moving) && x * x + y * y < r * r
            },
            |_| true,
        )
    }

    pub(in crate::game_world) fn collision_sweep(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
    ) -> bool {
        self.collision_sweep_bounded(index, from, to, stationary, repairing_spawn, None)
    }

    /// A movement-batch bound on neighbor radius and displacement. Padding
    /// requires strict separation, including already overlapping bodies.
    pub(in crate::game_world) fn collision_sweep_bounded(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
        bounds: Option<(i32, i32)>,
    ) -> bool {
        let unit = &self.units[index].state;
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let length = dx * dx + dy * dy;
        if length == 0 {
            return true;
        }
        let midpoint = WorldPosition::new(
            ((i64::from(from.x) + i64::from(to.x)) / 2) as i32,
            ((i64::from(from.y) + i64::from(to.y)) / 2) as i32,
        );
        let own_radius = unit.kind.collision_radius();
        let (maximum_radius, padding) = bounds.unwrap_or_else(|| {
            (
                self.collisions.maximum_radii()[usize::from(unit.kind.is_naval())],
                0,
            )
        });
        // Overflow keeps whole-tile bounds; otherwise use the half-step swept AABB.
        let reach = own_radius
            + if bounds.is_some() && !self.collisions.has_overfull_cells() {
                maximum_radius + padding + ((dx.abs().max(dy.abs()) + 1) / 2) as i32
            } else {
                768 + padding
            };
        // Melee may strike within reach while still approaching its preferred
        // point. Other traffic keeps full separation until its bounded squeeze.
        let attack = unit.attack_target;
        let crowded = attack
            .and_then(|target| self.attack_goal(target, unit.player))
            .is_some_and(|goal| self.melee_contact_traffic(unit.id, &goal));
        let soft = (self.units[index].formation.is_none() || self.is_detached_member(index))
            && (crowded
                || (attack.is_none()
                    && self.units[index].obstruction.is_some_and(|o| {
                        let (x, y) = (
                            i64::from(o.goal.x) - i64::from(from.x),
                            i64::from(o.goal.y) - i64::from(from.y),
                        );
                        o.pass_until > self.tick.0
                            && (x * x + y * y > 2048 * 2048
                                || self.free_work_approach(index, o.goal))
                    })));
        let work_squeeze = unit.kind == UnitKind::Villager
            && self.units[index].obstruction.is_some()
            && self.crowded_resource_approach(unit.id);
        let narrow_water = unit.kind.is_naval() && unit.moving && self.narrow_water_lane(from);
        // Every query can reject centres outside the swept box expanded by
        // the largest observed hull; overfull-cell policy remains independent.
        let (min_x, max_x, min_y, max_y) = {
            let broad_radius = i64::from(own_radius + maximum_radius + padding);
            (
                i64::from(from.x.min(to.x)) - broad_radius,
                i64::from(from.x.max(to.x)) + broad_radius,
                i64::from(from.y.min(to.y)) - broad_radius,
                i64::from(from.y.max(to.y)) + broad_radius,
            )
        };
        !self.collisions.blocked(
            (midpoint, reach),
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = &self.units[self.collisions.unit_index(id)].state;
                if i64::from(other.position.x) < min_x
                    || i64::from(other.position.x) > max_x
                    || i64::from(other.position.y) < min_y
                    || i64::from(other.position.y) > max_y
                {
                    return false;
                }

                if other.hp == 0 || (stationary && other.moving) {
                    return false;
                }
                let r = if !soft && !work_squeeze && !unit.kind.is_naval() {
                    i64::from(own_radius + other.kind.collision_radius())
                } else {
                    self.movement_contact_distance(unit, other, soft, narrow_water, work_squeeze)
                } + i64::from(padding);
                let x = i64::from(other.position.x) - i64::from(from.x);
                let y = i64::from(other.position.y) - i64::from(from.y);
                let start = x * x + y * y;
                let dot = x * dx + y * dy;
                // Spawn overlaps may separate without moving closer; separated bodies stay hard.
                if start < r * r {
                    if padding > 0 {
                        return true;
                    }
                    let x = i64::from(other.position.x) - i64::from(to.x);
                    let y = i64::from(other.position.y) - i64::from(to.y);
                    return !repairing_spawn && x * x + y * y < start;
                }
                if dot <= 0 {
                    false
                } else if dot >= length {
                    let x = i64::from(other.position.x) - i64::from(to.x);
                    let y = i64::from(other.position.y) - i64::from(to.y);
                    x * x + y * y < r * r
                } else {
                    let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
                    cross * cross < i128::from(r * r) * i128::from(length)
                }
            },
            |tile| {
                // More than 32 centres in one tile cannot form a separated packing
                // even for infantry. Treat it as a compressed spawn region: an
                // actor inside may escape radially; external traffic must route round.
                let cx = i64::from(tile.x) * 1024 + 512;
                let cy = i64::from(tile.y) * 1024 + 512;
                let x = i64::from(from.x) - cx;
                let y = i64::from(from.y) - cy;
                let box_distance = |at: WorldPosition| {
                    let x =
                        i64::from(at.x) - i64::from(at.x.clamp(tile.x * 1024, (tile.x + 1) * 1024));
                    let y =
                        i64::from(at.y) - i64::from(at.y.clamp(tile.y * 1024, (tile.y + 1) * 1024));
                    x * x + y * y
                };
                if from.tile_floor() == tile {
                    x * dx + y * dy < 0
                } else {
                    box_distance(to) <= box_distance(from)
                }
            },
        )
    }

    /// Circle against adjacent static tile footprints. At tile centres the
    /// largest supported radius fits without additional terrain queries.
    pub(crate) fn body_position_clear(&self, kind: UnitKind, at: WorldPosition) -> bool {
        if !self.config.valid_ground_position(at) {
            return false;
        }
        let r = kind.terrain_radius();
        let mut bridge = None;
        let here = at.tile_floor();
        let min = WorldPosition::new(at.x - r, at.y - r).tile_floor();
        let max = WorldPosition::new(at.x + r, at.y + r).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                self.group_work.collision(1);
                let tile = TileCoord::new(x, y);
                if tile == here {
                    continue;
                }
                let dx = i64::from(at.x) - i64::from(at.x.clamp(x * 1024, (x + 1) * 1024));
                let dy = i64::from(at.y) - i64::from(at.y.clamp(y * 1024, (y + 1) * 1024));
                if dx * dx + dy * dy < i64::from(r) * i64::from(r)
                    && (self.economy.blocked_tiles.contains(&tile)
                        || (!self
                            .unit_tile_passable(kind, tile, &|| false)
                            .unwrap_or(false)
                            && !(*bridge.get_or_insert_with(|| {
                                !kind.is_naval() && self.on_bridge_corridor(at, r)
                            }) && self.bridge_water_clear(tile))))
                {
                    return false;
                }
            }
        }
        true
    }
}
