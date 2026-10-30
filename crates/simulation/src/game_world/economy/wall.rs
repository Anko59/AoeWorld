//! Palisade and stone walls: a line of 1x1 segments laid by one order.
use super::{BuildingKind, EconomyError, MAX_WALL_SEGMENTS, placement::distance_squared};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};

/// Tiles on the straight (Bresenham) line between two tiles, ends included.
pub(super) fn line_tiles(from: TileCoord, to: TileCoord) -> Vec<TileCoord> {
    let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
    let (x1, y1) = (i64::from(to.x), i64::from(to.y));
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut error = dx + dy;
    let mut tiles = Vec::new();
    loop {
        tiles.push(TileCoord::new(x as i32, y as i32));
        if (x, y) == (x1, y1) || tiles.len() > MAX_WALL_SEGMENTS {
            return tiles;
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

impl GameWorld {
    /// Lay a wall of `kind` (palisade or stone) of 1x1 segments along the line `from..=to` and send the
    /// builders to the nearest segments. Returns the segment ids in line order.
    /// With `queued`, builders follow the segments in line order after existing work.
    pub fn issue_build_wall(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        kind: BuildingKind,
        from: TileCoord,
        to: TileCoord,
        queued: bool,
    ) -> Result<Vec<EntityId>, EconomyError> {
        if !kind.is_wall() {
            return Err(EconomyError::NotBuildable);
        }
        let builders = self.validate_builders(player, builders)?;
        let tiles = line_tiles(from, to);
        if tiles.len() > MAX_WALL_SEGMENTS {
            return Err(EconomyError::OrderTooLarge);
        }
        self.validate_build_queue(&builders, tiles.len(), queued)?;
        let cost = self.check_buildable(player, kind, tiles.len() as u32)?;
        let rects = tiles
            .iter()
            .map(|tile| {
                TileRect::new(
                    *tile,
                    TileCoord::new(tile.x.saturating_add(1), tile.y.saturating_add(1)),
                )
            })
            .collect::<Vec<_>>();
        // Validate every segment before placing any, so a refusal changes nothing.
        for rect in &rects {
            self.validate_explored_footprint(player, *rect)?;
            self.validate_footprint_ground(*rect)?;
        }
        let mut ids = Vec::new();
        for rect in rects {
            match self.insert_building(player, kind, rect, false) {
                Ok(id) => ids.push(id),
                Err(error) => {
                    for id in ids {
                        self.remove_building(id);
                    }
                    return Err(error);
                }
            }
        }
        self.charge(player, cost);
        for id in &ids {
            self.step_builders_aside(&builders, *id);
        }
        let segments = ids.clone();
        let nearest = |tile: TileCoord| {
            segments
                .iter()
                .copied()
                .min_by_key(|id| {
                    let rect = self.economy.buildings[id].footprint;
                    (distance_squared(tile, rect.min), *id)
                })
                .unwrap_or(segments[0])
        };
        let targets = builders
            .iter()
            .map(|id| {
                let tile = self.unit(*id).map_or(from, |u| u.position.tile_floor());
                (*id, nearest(tile))
            })
            .collect::<Vec<_>>();
        let assigned = if queued {
            self.assign_build_orders(&builders, &ids, true)
        } else {
            self.assign_builders(&builders, |builder| {
                targets
                    .iter()
                    .find(|(id, _)| *id == builder)
                    .map_or(ids[0], |(_, target)| *target)
            })
        };
        if !assigned {
            for id in &ids {
                let _ = self.issue_cancel_construction(player, *id);
            }
            return Err(crate::GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(ids)
    }
}
