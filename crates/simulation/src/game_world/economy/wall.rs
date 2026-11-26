//! Walls and roads: a line of 1x1 segments laid by one order.
use super::{BuildingKind, EconomyError, placement::distance_squared};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileLine, TileRect};
use std::collections::BTreeMap;

/// Exact nearest raster tile. Monotonic raster coordinates put every subrange
/// inside the rectangle of its endpoints, so a distant half can be skipped.
fn nearest_segment(line: TileLine, tile: TileCoord, open: &[usize]) -> usize {
    fn lower_bound(
        line: TileLine,
        tile: TileCoord,
        open: &[usize],
        (first, last): (usize, usize),
    ) -> (u64, usize) {
        let (a, b) = (line.tile(open[first]), line.tile(open[last]));
        let nearest = TileCoord::new(
            tile.x.clamp(a.x.min(b.x), a.x.max(b.x)),
            tile.y.clamp(a.y.min(b.y), a.y.max(b.y)),
        );
        (distance_squared(tile, nearest), open[first])
    }

    fn search(
        line: TileLine,
        tile: TileCoord,
        open: &[usize],
        (first, last): (usize, usize),
        bound: (u64, usize),
        best: &mut (u64, usize),
    ) {
        if bound >= *best {
            return;
        }
        if first == last {
            *best = bound;
            return;
        }
        let mid = first + (last - first) / 2;
        let a = (first, mid);
        let b = (mid + 1, last);
        let (a_dist, b_dist) = (
            lower_bound(line, tile, open, a),
            lower_bound(line, tile, open, b),
        );
        if a_dist <= b_dist {
            search(line, tile, open, a, a_dist, best);
            search(line, tile, open, b, b_dist, best);
        } else {
            search(line, tile, open, b, b_dist, best);
            search(line, tile, open, a, a_dist, best);
        }
    }

    let range = (0, open.len() - 1);
    let mut best = (u64::MAX, open[0]);
    search(line, tile, open, range, lower_bound(line, tile, open, range), &mut best);
    best.1
}

impl GameWorld {
    /// Lay 1x1 wall or road segments along `from..=to` and send the
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
        if !(kind.is_wall() || kind == BuildingKind::Road) {
            return Err(EconomyError::NotBuildable);
        }
        let builders = self.placement_builders(player, builders, false)?;
        let rect = |tile: TileCoord| TileRect::from_xywh(tile.x, tile.y, 1, 1);
        if ![from, to]
            .into_iter()
            .all(|tile| rect(tile).valid(self.config.width_tiles, self.config.height_tiles))
        {
            return Err(EconomyError::InvalidFootprint);
        }
        let line = TileLine::new(from, to).ok_or(EconomyError::InvalidFootprint)?;
        self.validate_build_queue(&builders, line.count(), queued)?;
        let cost = self.check_buildable(player, kind, line.count() as u32)?;
        if self.next_entity_id.saturating_add(line.count() as u64) > u64::from(u32::MAX) + 1 {
            return Err(crate::GameWorldError::EntityIdExhausted.into());
        }
        // Validate every segment before placing any, so a refusal changes nothing.
        for tile in line.tiles() {
            let footprint = rect(tile);
            self.validate_explored_footprint(player, footprint)?;
            self.validate_footprint_ground(player, footprint)?;
            self.validate_foundation_reach(player, &builders, footprint)?;
        }
        let ids = self.count_housing_once(|world| {
            let mut ids = Vec::with_capacity(line.count());
            for tile in line.tiles() {
                match world.insert_building(player, kind, rect(tile), false) {
                    Ok(id) => ids.push(id),
                    Err(error) => {
                        for id in ids {
                            world.remove_building(id);
                        }
                        return Err(error);
                    }
                }
            }
            Ok(ids)
        })?;
        self.charge(player, cost);
        if builders.is_empty() {
            self.queue_foundations(&ids, None);
            self.remember_visible_buildings();
            return Ok(ids);
        }
        let (open, blocked): (Vec<_>, Vec<_>) = (0..ids.len())
            .partition(|&segment| !self.foreign_foundation_blocker(ids[segment]));
        let blocked = blocked
            .into_iter()
            .map(|segment| ids[segment])
            .collect::<Vec<_>>();
        self.queue_foundations(&blocked, None);
        if open.is_empty() {
            self.remember_visible_buildings();
            return Ok(ids);
        }
        if kind.is_wall() {
            // Visit only the segments builders occupy. When a step leads to a
            // later segment it is visited again in the original placement order.
            let mut inside: BTreeMap<usize, Vec<EntityId>> = BTreeMap::new();
            for &id in &builders {
                if let Some(segment) = self
                    .unit(id)
                    .and_then(|u| line.index_of(u.position.tile_floor()))
                {
                    inside.entry(segment).or_default().push(id);
                }
            }
            while let Some((segment, mut local)) = inside.pop_first() {
                local.sort_unstable();
                if open.binary_search(&segment).is_err() {
                    continue;
                }
                self.step_builders_aside(&local, ids[segment]);
                for id in local {
                    if let Some(next) = self
                        .unit(id)
                        .and_then(|u| line.index_of(u.position.tile_floor()))
                        && next > segment
                    {
                        inside.entry(next).or_default().push(id);
                    }
                }
            }
        }
        let assigned = if queued {
            let targets = open.iter().map(|&segment| ids[segment]).collect::<Vec<_>>();
            self.assign_build_orders(&builders, &targets, true)
        } else {
            let targets: BTreeMap<_, _> = builders
                .iter()
                .map(|&id| {
                    let tile = self.unit(id).map_or(from, |u| u.position.tile_floor());
                    (id, ids[nearest_segment(line, tile, &open)])
                })
                .collect();
            self.assign_builders(&builders, |builder| targets[&builder])
        };
        if !assigned {
            self.count_housing_once(|world| {
                for id in &ids {
                    let _ = world.issue_cancel_construction(player, *id);
                }
            });
            return Err(crate::GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(ids)
    }
}
