//! Fine collision cells complement the wildlife index. Normal cells are exact;
//! overfull spawn cells admit outward escape, but never new arrivals.
use crate::{GameWorld, UnitKind, UnitTask};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

#[path = "collision/bridge.rs"]
mod bridge;

const CELL_LIMIT: usize = 32;
type Cells = HashMap<u64, Vec<EntityId>, BuildHasherDefault<CellHasher>>;

// Coordinates are internal integer keys. Mix the packed pair once instead of
// running the general-purpose keyed byte hasher on every neighbour probe.
#[derive(Debug, Default)]
pub(crate) struct CellHasher(u64);
impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23);
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
}

pub(crate) fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}

#[derive(Debug, Default)]
pub(crate) struct CollisionIndex {
    cells: [Cells; 2],
    slots: Vec<usize>,
    indices: Vec<usize>,
    recycled: Vec<Vec<EntityId>>,
    overfull_cells: usize,
    maximum_radii: [i32; 2],
}

impl CollisionIndex {
    /// Kinds are fixed at spawn. Retain the largest observed radius after
    /// removal: an upper bound avoids a population scan on every group tick.
    pub(crate) fn observe_kind(&mut self, kind: UnitKind) {
        let radius = &mut self.maximum_radii[usize::from(kind.is_naval())];
        *radius = (*radius).max(kind.collision_radius());
    }

    pub(super) fn maximum_radii(&self) -> [i32; 2] {
        self.maximum_radii
    }

    pub(crate) fn unit_index(&self, id: EntityId) -> usize {
        self.indices[id.0 as usize]
    }

    pub(crate) fn reindex(&mut self, id: EntityId, index: usize) {
        if let Some(slot) = self.indices.get_mut(id.0 as usize) {
            *slot = index;
        }
    }

    pub(crate) fn sampling_slot(&self, id: EntityId) -> u64 {
        self.slots
            .get(id.0 as usize)
            .filter(|&&n| n != usize::MAX)
            .map_or(u64::MAX, |&n| n as u64)
    }

    pub(crate) fn insert(&mut self, id: EntityId, tile: TileCoord, naval: bool, index: usize) {
        let at = id.0 as usize;
        if self.slots.get(at).is_some_and(|&slot| slot != usize::MAX) {
            return;
        }
        if at >= self.slots.len() {
            self.slots.resize(at + 1, usize::MAX);
            self.indices.resize(at + 1, usize::MAX);
        }
        self.indices[at] = index;
        let recycled = &mut self.recycled;
        let cell = self.cells[usize::from(naval)]
            .entry(key(tile))
            .or_insert_with(|| recycled.pop().unwrap_or_default());
        self.slots[at] = cell.len();
        cell.push(id);
        self.overfull_cells += usize::from(cell.len() == CELL_LIMIT + 1);
    }

    pub(crate) fn remove(&mut self, id: EntityId, tile: TileCoord, naval: bool) {
        let Some(&slot) = self
            .slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
        else {
            return;
        };
        let key = key(tile);
        let cells = &mut self.cells[usize::from(naval)];
        let Some(cell) = cells.get_mut(&key) else {
            return;
        };
        if cell.get(slot) != Some(&id) {
            return;
        }
        self.overfull_cells -= usize::from(cell.len() == CELL_LIMIT + 1);
        cell.swap_remove(slot);
        self.slots[id.0 as usize] = usize::MAX;
        if let Some(displaced) = cell.get(slot) {
            self.slots[displaced.0 as usize] = slot;
        }
        if cell.is_empty()
            && let Some(cell) = cells.remove(&key)
            && self.recycled.len() < 4096
        {
            self.recycled.push(cell);
        }
    }

    pub(crate) fn relocate(&mut self, id: EntityId, from: TileCoord, to: TileCoord, naval: bool) {
        if from != to {
            let index = self.unit_index(id);
            self.remove(id, from, naval);
            self.insert(id, to, naval, index);
        }
    }

    pub(super) fn has_overfull_cells(&self) -> bool {
        self.overfull_cells != 0
    }

    /// At most sixteen cells, with 32 body tests per cell. Hash iteration never
    /// determines a decision: cells and each cell's cyclic window are canonical.
    pub(super) fn blocked(
        &self,
        (at, reach): (WorldPosition, i32),
        naval: bool,
        salt: u64,
        work: &super::super::route_scheduler::GroupWork,
        mut test: impl FnMut(EntityId) -> bool,
        mut overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        let min =
            WorldPosition::new(at.x.saturating_sub(reach), at.y.saturating_sub(reach)).tile_floor();
        let max =
            WorldPosition::new(at.x.saturating_add(reach), at.y.saturating_add(reach)).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                work.collision(1);
                let tile = TileCoord::new(x, y);
                let Some(cell) = self.cells[usize::from(naval)].get(&key(tile)) else {
                    continue;
                };
                if cell.len() > CELL_LIMIT && overflow(tile) {
                    return true;
                }
                let len = cell.len();
                // The cyclic window starts at `salt % len`; a single body needs
                // no division and the walk wraps without a modulo per body.
                let mut at = if len == 1 {
                    0
                } else {
                    (salt % len as u64) as usize
                };
                for _ in 0..len.min(CELL_LIMIT) {
                    work.collision(1);
                    if test(cell[at]) {
                        return true;
                    }
                    at += 1;
                    if at == len {
                        at = 0;
                    }
                }
            }
        }
        false
    }
}

/// Hard shares of the radii sum: friendly traffic 50%, workers 75%, others 100%.
/// Ships keep whole hulls outside narrow-water and fishing contacts (contact.rs).
const HARD_FRIEND_MOVING: i64 = 50;
const HARD_FRIEND_IDLE: i64 = 50;
const HARD_FRIEND_WORKING: i64 = 75;
const HARD_OTHER: i64 = 100;

impl crate::GameUnit {
    /// A body working in place: friendly traffic keeps most of its distance.
    fn working(&self) -> bool {
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
    sum * percent / 100
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
                let r = self
                    .naval_worker_distance(&unit, &other)
                    .unwrap_or_else(|| {
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
        let (maximum_radius, padding) = bounds.unwrap_or((512, 0));
        // Overflow keeps whole-tile bounds; otherwise use the half-step swept AABB.
        let reach = own_radius
            + if bounds.is_some() && !self.collisions.has_overfull_cells() {
                maximum_radius + padding + ((dx.abs().max(dy.abs()) + 1) / 2) as i32
            } else {
                768 + padding
            };
        // Active attack jobs keep full-body approaches, including truthful Moving waits.
        let soft = !self.combat.attackers.contains_key(&unit.id)
            && self.units[index].formation.is_none()
            && self.units[index].obstruction.is_some_and(|o| {
                let (x, y) = (
                    i64::from(o.goal.x) - i64::from(from.x),
                    i64::from(o.goal.y) - i64::from(from.y),
                );
                o.pass_until > self.tick.0
                    && (x * x + y * y > 2048 * 2048 || self.free_work_approach(index, o.goal))
            });
        let narrow_water = unit.kind.is_naval() && unit.moving && self.narrow_water_lane(from);
        let culling = bounds.map(|_| {
            let broad_radius = i64::from(own_radius + maximum_radius + padding);
            (
                i64::from(from.x.min(to.x)) - broad_radius,
                i64::from(from.x.max(to.x)) + broad_radius,
                i64::from(from.y.min(to.y)) - broad_radius,
                i64::from(from.y.max(to.y)) + broad_radius,
            )
        });
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
                if let Some((min_x, max_x, min_y, max_y)) = culling
                    && (i64::from(other.position.x) < min_x
                        || i64::from(other.position.x) > max_x
                        || i64::from(other.position.y) < min_y
                        || i64::from(other.position.y) > max_y)
                {
                    return false;
                }

                if other.hp == 0 || (stationary && other.moving) {
                    return false;
                }
                let r = if let Some(r) = self.naval_worker_distance(&unit, &other) {
                    r
                } else if narrow_water && other.moving {
                    i64::from(own_radius + other.kind.collision_radius()) / 2
                } else if soft {
                    hard_distance(&unit, &other)
                } else {
                    i64::from(own_radius + other.kind.collision_radius())
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
