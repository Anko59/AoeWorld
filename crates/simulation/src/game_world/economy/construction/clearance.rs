//! Units on a foundation step aside; finished buildings never keep a body inside.
//!
//! Rule (as in AoE2): a foundation may be placed over units. Units of the
//! builder's player walk to the nearest free tile outside it by ordinary
//! movement and construction waits at the edge until the footprint is clear.
//! Units of other players are not ordered anywhere, so their presence keeps
//! the foundation from progressing until they leave or die. A unit that cannot
//! be routed off is placed on the nearest free tile instead.
use super::{BuildingKind, perimeter, position};
use crate::GameWorld;
use aoe_core::{EntityId, TileCoord, TileRect, WorldPosition};
use std::collections::BTreeSet;

/// Closest a unit centre may stand to a blocking footprint (movement's own
/// static-collision clearance, well inside the body radius), in subunits.
const EDGE_CLEARANCE: i64 = 128;

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

impl GameWorld {
    /// Nearest free walkable tile outside `footprint` for `unit`, skipping
    /// `taken`. The first two rings stay free for builders when possible.
    fn exit_tile(
        &self,
        id: EntityId,
        footprint: TileRect,
        taken: &BTreeSet<TileCoord>,
    ) -> Option<TileCoord> {
        let unit = self.unit(id)?;
        for margin in (1..8).chain(std::iter::once(0)) {
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

    /// Whether any unit still stands on the foundation. Idle units of the
    /// foundation's owner are sent off it; builders re-approach by themselves.
    pub(super) fn foundation_occupied(&mut self, building: EntityId) -> bool {
        let Some(b) = self.building(building) else {
            return false;
        };
        let owner = b.player;
        // Walkable kinds (farms, open gates) never trap or stall anyone.
        let Some(footprint) = b.kind.collision_rect(b.footprint, false) else {
            return false;
        };
        let inside = self
            .query(self.clamp_around(footprint))
            .0
            .into_iter()
            .filter(|u| overlaps(footprint, u))
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        for unit in &inside {
            if unit.player != owner || self.economy.builders.contains_key(&unit.id) {
                continue;
            }
            let Some(tile) = self.exit_tile(unit.id, footprint, &taken) else {
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
        }
        !inside.is_empty()
    }

    /// A builder found on its own foundation (it was pushed there) walks off
    /// before it can work or approach; true while it is still moving out.
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
        let Some(tile) = self.exit_tile(id, area, &BTreeSet::new()) else {
            return false;
        };
        let Ok(at) = position(tile) else {
            return false;
        };
        self.slide_toward(id, at, area);
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
            .filter(|u| overlaps(footprint, u))
            .map(|u| u.id)
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        for id in overlapping {
            if let Some(tile) = self.exit_tile(id, footprint, &taken)
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
                && let Some(tile) = self.exit_tile(*id, area, &taken)
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
