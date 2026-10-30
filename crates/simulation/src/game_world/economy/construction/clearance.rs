//! Units on a foundation step aside; finished buildings never keep a body inside.
//!
//! When construction completes, any unit whose body overlaps the new
//! footprint is placed on the nearest free walkable tile outside it.
use super::{BuildingKind, perimeter, position};
use crate::GameWorld;
use aoe_core::{EntityId, TileCoord, TileRect, WorldPosition};
use std::collections::BTreeSet;

/// Whether a unit's body (centre or radius) touches `footprint`.
fn overlaps(footprint: TileRect, u: &crate::GameUnit) -> bool {
    let r = i64::from(u.kind.collision_radius());
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
