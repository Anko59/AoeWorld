//! Palisade walls: a line of 1x1 segments laid by one order.
use super::{
    BuildTask, BuildingKind, EconomyError, MAX_WALL_SEGMENTS, UnitTask, placement::distance_squared,
};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};

/// A finished palisade segment sends its builders to unfinished neighbours
/// within this Chebyshev distance (nearest four tried).
const WALL_CONTINUE_RADIUS: i32 = 8;
/// Unfinished neighbours a builder tries before it gives up and idles.
const WALL_CONTINUE_TRIES: usize = 4;

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
    /// Lay a palisade of 1x1 segments along the line `from..=to` and send the
    /// builders to the nearest segments. Returns the segment ids in line order.
    pub fn issue_build_wall(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        from: TileCoord,
        to: TileCoord,
    ) -> Result<Vec<EntityId>, EconomyError> {
        let builders = self.validate_builders(player, builders)?;
        let tiles = line_tiles(from, to);
        if tiles.len() > MAX_WALL_SEGMENTS {
            return Err(EconomyError::Capacity);
        }
        let kind = BuildingKind::PalisadeWall;
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
            self.validate_footprint(*rect)?;
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
        if !self.assign_builders(&builders, |builder| {
            targets
                .iter()
                .find(|(id, _)| *id == builder)
                .map_or(ids[0], |(_, target)| *target)
        }) {
            for id in &ids {
                let _ = self.issue_cancel_construction(player, *id);
            }
            return Err(crate::GameWorldError::Unreachable.into());
        }
        for id in &ids {
            self.update_building_vision(*id);
        }
        self.remember_visible_buildings();
        Ok(ids)
    }

    pub(super) fn continue_wall(
        &mut self,
        builder: EntityId,
        player: PlayerId,
        finished: TileRect,
    ) {
        let mut next = self
            .buildings()
            .filter(|b| {
                b.player == player
                    && b.kind == BuildingKind::PalisadeWall
                    && !b.is_complete()
                    && b.footprint.min.x.abs_diff(finished.min.x) <= WALL_CONTINUE_RADIUS as u32
                    && b.footprint.min.y.abs_diff(finished.min.y) <= WALL_CONTINUE_RADIUS as u32
            })
            .map(|b| {
                (
                    distance_squared(b.footprint.min, finished.min),
                    b.id,
                    b.footprint,
                )
            })
            .collect::<Vec<_>>();
        next.sort_by_key(|(distance, id, _)| (*distance, *id));
        let nearest = next.first().map(|(_, building, _)| *building);
        for (_, building, footprint) in next.into_iter().take(WALL_CONTINUE_TRIES) {
            if self.approach_building(builder, footprint, 0).is_err() {
                continue;
            }
            self.economy.builders.insert(
                builder,
                BuildTask {
                    building,
                    attempts: 0,
                },
            );
            if let Some(&index) = self.lookup.get(&builder) {
                self.units[index].state.task = UnitTask::Moving;
            }
            return;
        }
        // Every approach failed this tick (for instance the path budget was
        // spent): stay assigned to the nearest segment and retry next tick.
        if let Some(building) = nearest {
            self.economy.builders.insert(
                builder,
                BuildTask {
                    building,
                    attempts: 1,
                },
            );
        }
    }
}
