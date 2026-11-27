//! Units on a foundation step aside; finished buildings never keep a body inside.
//!
//! Rule (as in AoE2): a foundation may be placed over units. Units of the
//! builder's player walk to the nearest free tile outside it by ordinary
//! movement and construction waits at the edge until the footprint is clear.
//! A foreign unit suspends the foundation and releases its crew. The plan
//! remains available when the footprint clears. An assigned builder found
//! inside its own foundation is placed on the nearest free work tile.
use super::{BuildingKind, perimeter, position};
use crate::GameWorld;
use aoe_core::{EntityId, TileCoord, TileRect, WorldPosition};
use std::collections::BTreeSet;

/// Closest a unit centre may stand to a blocking footprint (movement's own
/// static-collision clearance, well inside the body radius), in subunits.
const EDGE_CLEARANCE: i64 = 128;

/// Rings around a footprint searched for a free tile to stand on.
const EXIT_RINGS: i32 = 32;

/// Whether a unit stands in `footprint` or closer to its edge than movement
/// allows.
fn overlaps(footprint: TileRect, u: &crate::GameUnit) -> bool {
    let r = EDGE_CLEARANCE;
    let (x, y) = (i64::from(u.position.x), i64::from(u.position.y));
    let dx = (i64::from(footprint.min.x) * 1024 - x)
        .max(x - i64::from(footprint.max.x) * 1024)
        .max(0);
    let dy = (i64::from(footprint.min.y) * 1024 - y)
        .max(y - i64::from(footprint.max.y) * 1024)
        .max(0);
    footprint.contains(u.position.tile_floor()) || dx * dx + dy * dy < r * r
}

pub(super) enum Obstruction {
    Clearing,
    Foreign,
}

impl GameWorld {
    /// A pending foundation is staffed only when its footprint is clear.
    pub(crate) fn foundation_has_blocker(&self, id: EntityId) -> bool {
        self.foundation_blocker(id, false, None)
    }

    pub(in super::super) fn foreign_foundation_blocker(&self, id: EntityId) -> bool {
        self.foundation_blocker(id, true, None)
    }

    /// A continuing builder may already touch its next work site.
    pub(super) fn foundation_has_other_blocker(&self, id: EntityId, builder: EntityId) -> bool {
        self.foundation_blocker(id, false, Some(builder))
    }

    fn foundation_blocker(
        &self,
        id: EntityId,
        foreign_only: bool,
        continuing: Option<EntityId>,
    ) -> bool {
        let Some(b) = self.economy.buildings.get(&id).filter(|b| !b.is_complete()) else {
            return false;
        };
        let Some(rect) = b.kind.collision_rect(b.footprint, false) else {
            return false;
        };
        let around = self.clamp_around(rect);
        if around.min.x >= around.max.x || around.min.y >= around.max.y {
            return false;
        }
        let max = TileCoord::new(around.max.x - 1, around.max.y - 1);
        [false, true].into_iter().any(|naval| {
            self.collisions
                .census_any(around.min, max, naval, |unit_id, _| {
                    if Some(unit_id) == continuing {
                        return false;
                    }
                    self.unit(unit_id).is_some_and(|u| {
                        u.hp > 0
                            && (!foreign_only || u.player != b.player)
                            && overlaps(rect, &u)
                            && !self.embarked(u.id)
                            && self
                                .economy
                                .builders
                                .get(&u.id)
                                .is_none_or(|job| job.building != id)
                    })
                })
        })
    }

    pub(super) fn suspend_occupied_foundation(
        &mut self,
        id: EntityId,
        changed: &mut Vec<EntityId>,
    ) {
        self.wake_construction(id);
        self.economy.dispatch.sites.entry(id).or_insert(None);
        if let Some(crew) = self.economy.builders.members.get(&id).cloned() {
            for builder in crew {
                if let Some(&index) = self.lookup.get(&builder) {
                    self.stop_movement(index);
                }
                if !self.next_queued_build(builder)
                    && self.economy.dispatch.lent.contains_key(&builder)
                    && let Some(site) = self.building(id)
                {
                    self.after_building(builder, site.player, id, site.kind, site.footprint);
                }
                changed.push(builder);
            }
        }
    }

    /// Nearest free walkable tile outside `footprint` for `unit`, skipping
    /// `taken`, searched ring by ring outward from the footprint edge (up to
    /// `EXIT_RINGS`). With `spare_edge` the first ring is tried last, so idle
    /// bystanders leave the builders' work spots free when they can.
    fn exit_tile(
        &self,
        id: EntityId,
        footprint: TileRect,
        taken: &BTreeSet<TileCoord>,
        spare_edge: bool,
    ) -> Option<TileCoord> {
        let unit = self.unit(id)?;
        let margins: Vec<i32> = if spare_edge {
            (1..EXIT_RINGS).chain(std::iter::once(0)).collect()
        } else {
            (0..EXIT_RINGS).collect()
        };
        for margin in margins {
            let ring = perimeter(TileRect::from_xywh(
                footprint.min.x - margin,
                footprint.min.y - margin,
                footprint.width() + 2 * margin,
                footprint.height() + 2 * margin,
            ));
            let best = ring
                .into_iter()
                .filter(|tile| {
                    !taken.contains(tile)
                        && self.tile_free(*tile)
                        && self
                            .unit_tile_passable(unit.kind, *tile, &|| false)
                            .unwrap_or(false)
                })
                .min_by_key(|tile| {
                    let at = position(*tile).unwrap_or(unit.position);
                    let (x, y) = (
                        i64::from(at.x) - i64::from(unit.position.x),
                        i64::from(at.y) - i64::from(unit.position.y),
                    );
                    (x * x + y * y, tile.y, tile.x)
                });
            if best.is_some() {
                return best;
            }
        }
        None
    }

    /// Move friendly bystanders off the footprint and report obstructions.
    pub(super) fn clear_foundation(
        &mut self,
        building: EntityId,
        changed: &mut Vec<EntityId>,
    ) -> Option<Obstruction> {
        let b = self.economy.buildings.get(&building)?;
        if b.is_complete() {
            return None;
        }
        let owner = b.player;
        // Walkable kinds such as farms and roads never block construction.
        let footprint = b.kind.collision_rect(b.footprint, false)?;
        let inside = self
            // Corpses neither block a foundation nor get walked off it.
            .living_units_on_tiles(self.clamp_around(footprint))
            .into_iter()
            .filter(|u| overlaps(footprint, u))
            // This foundation's own builders never block it: pressed against
            // the edge they still work, and any on the footprint are lifted
            // off by `builder_leaves_foundation`.
            .filter(|u| {
                self.economy
                    .builders
                    .get(&u.id)
                    .is_none_or(|task| task.building != building)
            })
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        for unit in &inside {
            if unit.player != owner || self.economy.builders.contains_key(&unit.id) {
                continue;
            }
            let Some(tile) = self.exit_tile(unit.id, footprint, &taken, true) else {
                continue;
            };
            taken.insert(tile);
            let Ok(at) = position(tile) else { continue };
            if unit.task != crate::UnitTask::Moving {
                self.stop_gathering(unit.id);
                self.cancel_attack(unit.id);
                if let Some(&index) = self.lookup.get(&unit.id) {
                    self.stop_movement(index);
                }
            }
            self.slide_toward(unit.id, at, footprint);
            changed.push(unit.id);
        }
        if inside.iter().any(|u| u.player != owner) {
            Some(Obstruction::Foreign)
        } else {
            (!inside.is_empty()).then_some(Obstruction::Clearing)
        }
    }

    /// A builder found on its own foundation (it was pushed there) is set on
    /// the nearest free tile outside it, then approaches and works as usual.
    /// Returns whether it was moved (it then waits for the next tick).
    pub(super) fn builder_leaves_foundation(&mut self, id: EntityId, building: EntityId) -> bool {
        let Some(b) = self.building(building) else {
            return false;
        };
        let Some(area) = b.kind.collision_rect(b.footprint, false) else {
            return false;
        };
        if b.is_complete()
            || !self
                .unit(id)
                .is_some_and(|u| area.contains(u.position.tile_floor()))
        {
            return false;
        }
        let Some(tile) = self.exit_tile(id, area, &BTreeSet::new(), false) else {
            return false;
        };
        let Ok(at) = position(tile) else {
            return false;
        };
        // Placed, not walked: a crowd of builders would push a walker back.
        self.displace_unit(id, at);
        true
    }

    /// Place every unit whose body overlaps a just-finished `footprint` on the
    /// nearest free tile outside it.
    pub(super) fn eject_from_footprint(&mut self, kind: BuildingKind, footprint: TileRect) {
        let Some(footprint) = kind.collision_rect(footprint, true) else {
            return;
        };
        let around = self.clamp_around(footprint);
        let overlapping = self
            .query(around)
            .0
            .into_iter()
            .filter(|u| u.hp > 0 && overlaps(footprint, u))
            .map(|u| u.id)
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        for id in overlapping {
            if let Some(tile) = self.exit_tile(id, footprint, &taken, false)
                && let Ok(at) = position(tile)
            {
                taken.insert(tile);
                self.displace_unit(id, at);
            }
        }
    }

    /// Builders standing where they just placed a foundation cannot route out
    /// of the blocked footprint, so they step to the nearest free tile first.
    pub(in crate::game_world) fn step_builders_aside(
        &mut self,
        builders: &[EntityId],
        foundation: EntityId,
    ) {
        let Some(b) = self.building(foundation) else {
            return;
        };
        let Some(area) = b.kind.collision_rect(b.footprint, false) else {
            return;
        };
        let mut taken = BTreeSet::new();
        for id in builders {
            if self
                .unit(*id)
                .is_some_and(|u| area.contains(u.position.tile_floor()))
                && let Some(tile) = self.exit_tile(*id, area, &taken, false)
                && let Ok(at) = position(tile)
            {
                taken.insert(tile);
                self.displace_unit(*id, at);
            }
        }
    }

    /// One walking step toward `to`, ignoring the blocked footprint the unit
    /// stands in: the route planner and collision step cannot start on a
    /// blocked tile, and the way out is a few tiles straight across it.
    fn slide_toward(&mut self, id: EntityId, to: WorldPosition, footprint: TileRect) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let from = self.units[index].state.position;
        let (dx, dy) = (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        );
        let distance = (dx * dx + dy * dy).isqrt();
        let step = (i64::from(self.config.move_speed_subunits_per_tick)
            / i64::try_from(self.config.move_speed_subunits_per_tick_denominator)
                .unwrap_or(1)
                .max(1))
        .max(1);
        let next = if distance <= step {
            to
        } else {
            WorldPosition::new(
                (i64::from(from.x) + dx * step / distance) as i32,
                (i64::from(from.y) + dy * step / distance) as i32,
            )
        };
        let previous = from.tile_floor();
        let unit = &mut self.units[index];
        unit.state.previous_position = from;
        unit.state.position = next;
        unit.state.facing = crate::game_world::facing_for(dx, dy, unit.state.facing);
        let clear = !overlaps(footprint, &unit.state);
        unit.state.moving = !clear;
        unit.state.task = if clear {
            crate::UnitTask::Idle
        } else {
            crate::UnitTask::Moving
        };
        let state = unit.state;
        self.track_unit_step(id, previous, &state);
    }

    /// The living units of `query(rect)`, read from the 1-tile collision
    /// cells instead of the 16-tile chunk buckets: a foundation checked every
    /// tick in a crowded town no longer copies every unit of its chunks
    /// (#978). Corpses leave the cells at death. Same units, in id order
    /// (exit tiles are handed out in that order).
    fn living_units_on_tiles(&self, rect: TileRect) -> Vec<crate::GameUnit> {
        let mut ids = Vec::new();
        if rect.min.x < rect.max.x && rect.min.y < rect.max.y {
            let max = TileCoord::new(rect.max.x - 1, rect.max.y - 1);
            for naval in [false, true] {
                self.collisions.census_any(rect.min, max, naval, |id, _| {
                    ids.push(id);
                    false
                });
            }
        }
        ids.sort_unstable();
        ids.dedup();
        let units = ids
            .into_iter()
            .filter_map(|id| self.unit(id))
            .filter(|unit| unit.hp > 0 && !self.embarked(unit.id))
            .collect::<Vec<_>>();
        debug_assert_eq!(
            units.iter().map(|u| u.id).collect::<Vec<_>>(),
            self.query(rect)
                .0
                .iter()
                .filter(|u| u.hp > 0)
                .map(|u| u.id)
                .collect::<Vec<_>>(),
            "collision cells disagree with the chunk query over {rect:?}"
        );
        units
    }

    /// `rect` grown by one tile and clipped to the map.
    fn clamp_around(&self, rect: TileRect) -> TileRect {
        TileRect::new(
            TileCoord::new((rect.min.x - 1).max(0), (rect.min.y - 1).max(0)),
            TileCoord::new(
                (rect.max.x + 1).min(self.config.width_tiles),
                (rect.max.y + 1).min(self.config.height_tiles),
            ),
        )
    }

    /// Put a unit on `to`, ending its movement; used where ordinary routing
    /// cannot start inside a blocked footprint.
    fn displace_unit(&mut self, id: EntityId, to: WorldPosition) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let from = self.units[index].state.position.tile_floor();
        self.stop_movement(index);
        self.units[index].state.position = to;
        self.units[index].state.previous_position = to;
        let state = self.units[index].state;
        self.track_unit_step(id, from, &state);
    }
}
