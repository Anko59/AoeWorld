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

#[derive(Clone, Copy)]
struct SweepPolicy {
    general_alone: bool,
    file: Option<aoe_core::EntityId>,
    soft: bool,
    work_squeeze: bool,
    narrow_water: bool,
    blocked: u16,
}

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
                    | UnitTask::Seeding
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
    /// Persistent queues behind another group's parked bodies need a local
    /// detour. A group's own files retain their shared packing and route.
    pub(crate) fn stationary_external_traffic(&self, index: usize) -> bool {
        self.stationary_external_body(index, false)
    }

    pub(crate) fn stationary_work_traffic(&self, index: usize) -> bool {
        self.stationary_external_body(index, true)
    }

    fn stationary_external_body(&self, index: usize, working_only: bool) -> bool {
        let unit = &self.units[index];
        self.collisions.blocked(
            (
                unit.state.position,
                unit.state.kind.collision_radius() + 1024,
            ),
            unit.state.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.state.id.0)),
            &self.group_work,
            |id| {
                let other = &self.units[self.collisions.unit_index(id)];
                let r = i64::from(
                    unit.state.kind.collision_radius() + other.state.kind.collision_radius() + 128,
                );
                let dx = i64::from(unit.state.position.x) - i64::from(other.state.position.x);
                let dy = i64::from(unit.state.position.y) - i64::from(other.state.position.y);
                id != unit.state.id
                    && other.state.hp > 0
                    && !other.state.moving
                    && (!working_only || other.state.working())
                    && (unit.formation.is_none() || other.formation != unit.formation)
                    && dx * dx + dy * dy < r * r
            },
            |_| true,
        )
    }

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

    pub(in crate::game_world) fn collision_sweep_bounded(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
        bounds: Option<(i32, i32)>,
    ) -> bool {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("body_sweep"));
        self.collision_sweep_impl::<true>(index, from, to, stationary, repairing_spawn, bounds)
    }

    /// A movement-batch bound on neighbor radius and displacement. Padding
    /// requires strict separation, including already overlapping bodies.
    fn collision_sweep_impl<const FAST: bool>(
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
        let swept = maximum_radius + padding + ((dx.abs().max(dy.abs()) + 1) / 2) as i32;
        let reach = own_radius
            + if self.collisions.has_overfull_cells() {
                768 + padding
            } else if bounds.is_some() {
                swept
            } else {
                // Every body outside the swept box is rejected below, so cells
                // beyond it hold no answer: skipping them changes no result.
                swept.min(768 + padding)
            };
        // Fuse the cell proof with the normal traversal. Empty/actor-only
        // cells and out-of-window centres never construct contact policy;
        // crowded queries do not pay for a second scan of the same cells.
        let mut policy = (!FAST).then(|| self.sweep_policy(index, from, unit));
        let mut narrow = None;
        // Every query can reject centres outside the swept box expanded by
        // the largest observed hull; overfull-cell policy remains independent.
        let window = {
            let broad_radius = i64::from(own_radius + maximum_radius + padding);
            let clamp = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            (
                clamp(i64::from(from.x.min(to.x)) - broad_radius),
                clamp(i64::from(from.x.max(to.x)) + broad_radius),
                clamp(i64::from(from.y.min(to.y)) - broad_radius),
                clamp(i64::from(from.y.max(to.y)) + broad_radius),
            )
        };
        let clear = !self.collisions.blocked_in(
            (midpoint, reach),
            window,
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = &self.units[self.collisions.unit_index(id)].state;
                if other.hp == 0 || (stationary && other.moving) {
                    return false;
                }
                let SweepPolicy {
                    general_alone,
                    file,
                    soft,
                    work_squeeze,
                    narrow_water,
                    blocked,
                } = *policy.get_or_insert_with(|| self.sweep_policy(index, from, unit));
                // His own army yields to a General the player walks alone.
                if general_alone && self.is_general_follower(other.id, unit.id) {
                    return false;
                }
                if file.is_some() && other.player == unit.player {
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
                let hit = if start < r * r {
                    padding > 0 || {
                        let x = i64::from(other.position.x) - i64::from(to.x);
                        let y = i64::from(other.position.y) - i64::from(to.y);
                        !repairing_spawn && x * x + y * y < start
                    }
                } else if dot <= 0 {
                    false
                } else if dot >= length {
                    let x = i64::from(other.position.x) - i64::from(to.x);
                    let y = i64::from(other.position.y) - i64::from(to.y);
                    x * x + y * y < r * r
                } else {
                    let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
                    cross * cross < i128::from(r * r) * i128::from(length)
                };
                hit && !(blocked >= 16
                    && unit.task == UnitTask::Moving
                    && other.task == UnitTask::Moving
                    && other.player == unit.player
                    && !other.kind.is_naval()
                    && !unit.kind.is_naval()
                    && (self.units[self.collisions.unit_index(id)].formation
                        != self.units[index].formation
                        || self.units[index].formation.is_none())
                    && self
                        .deck_heading(self.collisions.unit_index(id))
                        .is_some_and(|(x, y)| x * dx + y * dy < 0)
                    && *narrow.get_or_insert_with(|| self.passing_lane(index, from, blocked))
                    && self.passing_lane(self.collisions.unit_index(id), other.position, u16::MAX))
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
        );
        if FAST && policy.is_none() {
            if let Some(full) =
                self.sweep_oracle(index, from, to, stationary, repairing_spawn, bounds)
            {
                assert_eq!(clear, full);
            }
            if let Some(probe) = &self.planning_probe {
                probe.movement_count("body_policy_skipped", 1);
            }
        }
        clear
    }

    fn sweep_policy(
        &self,
        index: usize,
        from: WorldPosition,
        unit: &crate::GameUnit,
    ) -> SweepPolicy {
        // Melee may strike within reach while still approaching its preferred
        // point. Other traffic keeps full separation until its bounded squeeze.
        let attack = unit.attack_target;
        let crowded = attack
            .and_then(|target| self.attack_goal(target, unit.player))
            .is_some_and(|goal| self.melee_contact_traffic(unit.id, &goal));
        // A persistently blocked group member earns the same bounded friendly
        // passing clearance as an independent mover. Group membership must not
        // make parked friendly bodies an unbreakable wall.
        let obstruction = self.units[index].obstruction;
        let soft = crowded
            || (attack.is_none()
                && obstruction.is_some_and(|o| {
                    let (x, y) = (
                        i64::from(o.goal.x) - i64::from(from.x),
                        i64::from(o.goal.y) - i64::from(from.y),
                    );
                    o.pass_until > self.tick.0
                        && (x * x + y * y > 2048 * 2048
                            || self
                                .formation_destination_slot(unit.id)
                                .is_some_and(|goal| {
                                    let dx = i64::from(goal.x) - i64::from(from.x);
                                    let dy = i64::from(goal.y) - i64::from(from.y);
                                    dx * dx + dy * dy > 2048 * 2048
                                })
                            || self.free_work_approach(index, o.goal))
                }));
        let general_alone = unit.kind == UnitKind::General && self.units[index].formation.is_none();
        let work_squeeze = unit.kind == UnitKind::Villager
            && obstruction.is_some()
            && self.crowded_resource_approach(unit.id);
        let narrow_water = unit.kind.is_naval() && unit.moving && self.narrow_water_lane(from);
        // On a deck or in a one-lane passage a player's own bodies that are
        // not travelling the same way (oncoming or standing) pass through each
        // other, so a head-on meeting can never lock the lane for anyone.
        // Inside a one-lane passage a single file's own members pass through
        // each other: a knot of friendly bodies at the mouth cannot lock it.
        // Both passes are last resorts for a body that has stayed blocked, so
        // the group lookup waits for 16 blocked ticks (the same answer).
        let blocked = obstruction.map_or(0, |o| o.blocked);
        let file = (blocked >= 16)
            .then(|| self.single_file_group(index))
            .flatten()
            .filter(|_| {
                self.near_deck(from)
                    || (blocked >= 64 && self.static_clearance(unit.kind, from.tile_floor()) <= 1)
            });
        SweepPolicy {
            general_alone,
            file,
            soft,
            work_squeeze,
            narrow_water,
            blocked,
        }
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

#[cfg(not(test))]
impl GameWorld {
    #[inline(always)]
    fn sweep_oracle(
        &self,
        _: usize,
        _: WorldPosition,
        _: WorldPosition,
        _: bool,
        _: bool,
        _: Option<(i32, i32)>,
    ) -> Option<bool> {
        None
    }
}

#[cfg(test)]
impl GameWorld {
    pub(super) fn collision_sweep_full(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
        bounds: Option<(i32, i32)>,
    ) -> bool {
        self.collision_sweep_impl::<false>(index, from, to, stationary, repairing_spawn, bounds)
    }
    fn sweep_oracle(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
        bounds: Option<(i32, i32)>,
    ) -> Option<bool> {
        Some(crate::game_world::route_scheduler::GroupWork::unmeasured(
            || self.collision_sweep_full(index, from, to, stationary, repairing_spawn, bounds),
        ))
    }
}
