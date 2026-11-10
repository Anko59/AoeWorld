//! Territory split between one player's Steward TownCenters: a point belongs to the
//! nearest active town_center within the radius (integer squared distance), ties to
//! the lower Steward ID. No point ever belongs to two Stewards.
use super::{Home, STEWARD_RULES};
use crate::{GameWorld, Terrain};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, TileCoord, TileRect, WorldPosition};
use std::collections::VecDeque;
use std::sync::Arc;

fn d2(a: WorldPosition, b: WorldPosition) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}
impl Home {
    /// Inside the circle and not closer to (or tied but lower-ID than) another
    /// active town_center of the same player.
    pub(crate) fn owns(&self, at: WorldPosition) -> bool {
        self.contains(at) && {
            let mine = d2(self.center, at);
            !self.rivals.iter().any(|&(id, c)| {
                let theirs = d2(c, at);
                theirs < mine || (theirs == mine && id < self.id)
            })
        }
    }
}
impl GameWorld {
    /// Recompute each active domain's nearby rivals for one player. Runs only
    /// when a domain activates or deactivates, never per tick.
    pub(super) fn refresh_steward_territory(&mut self, player: PlayerId) {
        let members: Vec<(EntityId, WorldPosition)> = self
            .economy
            .stewards
            .by_player
            .get(&player)
            .map(|m| m.iter().map(|(id, at)| (*id, *at)).collect())
            .unwrap_or_default();
        let reach = i64::from(2 * STEWARD_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        for &(id, center) in &members {
            let rivals = members
                .iter()
                .filter(|&&(other, at)| other != id && d2(center, at) <= reach * reach)
                .copied()
                .collect();
            if let Some(home) = self.economy.steward_homes.get_mut(&id) {
                self.economy
                    .stewards
                    .controlled
                    .mark(home.workers.iter().chain(&home.navy.ships).copied());
                home.rivals = rivals;
                home.regions = None;
            }
        }
    }
}

// Connectivity inside one Steward's territory. A job is only worth giving a
// villager when a walk exists that never leaves the 100-tile circle (cut by its
// siblings). Static ground is labelled once per territory and bridge change;
// each query is then a few array reads, never a route search.
#[derive(Debug)]
pub(super) struct Regions {
    epoch: u32,
    min: TileCoord,
    side: i32,
    /// 0 outside the territory or on closed ground, otherwise a component id.
    labels: Vec<u16>,
}

impl Regions {
    fn label(&self, tile: TileCoord) -> u16 {
        let (x, y) = (tile.x - self.min.x, tile.y - self.min.y);
        if x < 0 || y < 0 || x >= self.side || y >= self.side {
            return 0;
        }
        self.labels[(y * self.side + x) as usize]
    }

    fn label_near(&self, tile: TileCoord) -> u16 {
        for r in 0..=1 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let label = self.label(TileCoord::new(tile.x + dx, tile.y + dy));
                    if label != 0 {
                        return label;
                    }
                }
            }
        }
        0
    }

    /// Can a walker at `from` reach the tiles touching `target` without
    /// leaving the territory? Unknown starting ground is allowed.
    pub(super) fn connects(&self, from: TileCoord, target: TileRect) -> bool {
        let start = self.label_near(from);
        if start == 0 {
            return true;
        }
        (target.min.y - 1..=target.max.y).any(|y| {
            (target.min.x - 1..=target.max.x).any(|x| self.label(TileCoord::new(x, y)) == start)
        })
    }
}

impl Home {
    pub(super) fn connects(&self, from: TileCoord, target: TileRect) -> bool {
        self.regions
            .as_ref()
            .is_none_or(|regions| regions.connects(from, target))
    }
    pub(super) fn connects_tile(&self, from: TileCoord, target: TileCoord) -> bool {
        self.connects(
            from,
            TileRect::new(target, TileCoord::new(target.x + 1, target.y + 1)),
        )
    }
}

impl GameWorld {
    /// Builds the labels once; later passes only compare the epoch.
    pub(super) fn steward_ensure_regions(&mut self, id: EntityId) {
        let epoch = self.economy.stewards.bridge_epoch;
        let Some(home) = self.economy.steward_homes.get(&id) else {
            return;
        };
        if !matches!(self.terrain, Terrain::Map { .. })
            || home.regions.as_ref().is_some_and(|r| r.epoch == epoch)
        {
            return;
        }
        let regions = Arc::new(self.label_territory(home, epoch));
        if let Some(home) = self.economy.steward_homes.get_mut(&id) {
            home.regions = Some(regions);
        }
    }

    fn label_territory(&self, home: &Home, epoch: u32) -> Regions {
        let r = STEWARD_RULES.radius_tiles;
        let c = home.center.tile_floor();
        let side = 2 * r + 1;
        let min = TileCoord::new(c.x - r, c.y - r);
        let mut labels = vec![0u16; (side * side) as usize];
        let inside = |tile: TileCoord| {
            home.owns(WorldPosition::new(
                tile.x * FIXED_SUBUNITS_PER_TILE + FIXED_SUBUNITS_PER_TILE / 2,
                tile.y * FIXED_SUBUNITS_PER_TILE + FIXED_SUBUNITS_PER_TILE / 2,
            )) && tile.x >= 0
                && tile.y >= 0
                && tile.x < self.config.width_tiles
                && tile.y < self.config.height_tiles
        };
        let index = |tile: TileCoord| ((tile.y - min.y) * side + (tile.x - min.x)) as usize;
        let mut next = 0u16;
        for y in min.y..min.y + side {
            for x in min.x..min.x + side {
                let start = TileCoord::new(x, y);
                if labels[index(start)] != 0 || !inside(start) {
                    continue;
                }
                // Closed ground is its own unlabelled tile; open ground floods.
                if !self.ground_open(start) {
                    continue;
                }
                next = next.saturating_add(1);
                labels[index(start)] = next;
                let mut queue = VecDeque::from([start]);
                while let Some(from) = queue.pop_front() {
                    for (dx, dy) in [
                        (0, -1),
                        (-1, 0),
                        (1, 0),
                        (0, 1),
                        (-1, -1),
                        (1, -1),
                        (-1, 1),
                        (1, 1),
                    ] {
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if to.x < min.x
                            || to.y < min.y
                            || to.x >= min.x + side
                            || to.y >= min.y + side
                            || labels[index(to)] != 0
                            || !inside(to)
                            || !self.terrain.ground_step_open(from, to)
                        {
                            continue;
                        }
                        labels[index(to)] = next;
                        queue.push_back(to);
                    }
                }
            }
        }
        Regions {
            epoch,
            min,
            side,
            labels,
        }
    }

    fn ground_open(&self, tile: TileCoord) -> bool {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return true;
        };
        generator
            .walk_at_with_cancel(tile, &|| false)
            .ok()
            .flatten()
            .is_some_and(|cell| cell.passable || overlay.bridge_connections(tile).is_some())
    }
}

#[cfg(test)]
impl GameWorld {
    /// Test hook: label every tile within `margin` of `rect` as its own
    /// component, or restore ordinary labelling when `rect` is `None`.
    pub(crate) fn steward_cut_territory(&mut self, id: EntityId, rect: Option<TileRect>) {
        self.steward_ensure_regions(id);
        let Some(home) = self.economy.steward_homes.get_mut(&id) else {
            return;
        };
        let Some(rect) = rect else {
            home.regions = None;
            return;
        };
        let Some(old) = home.regions.as_ref() else {
            return;
        };
        let mut labels = old.labels.clone();
        for y in 0..old.side {
            for x in 0..old.side {
                let (tx, ty) = (old.min.x + x, old.min.y + y);
                let near = tx >= rect.min.x - 6
                    && tx <= rect.max.x + 6
                    && ty >= rect.min.y - 6
                    && ty <= rect.max.y + 6;
                if near && labels[(y * old.side + x) as usize] != 0 {
                    labels[(y * old.side + x) as usize] = u16::MAX;
                }
            }
        }
        home.regions = Some(Arc::new(Regions {
            epoch: old.epoch,
            min: old.min,
            side: old.side,
            labels,
        }));
    }
}

impl GameWorld {
    pub(crate) fn steward_land_sources(&self) -> Vec<(PlayerId, aoe_core::TileCoord, i32)> {
        self.economy
            .steward_homes
            .values()
            .filter(|h| h.town_center.is_some())
            .map(|h| (h.player, h.center.tile_floor(), STEWARD_RULES.radius_tiles))
            .collect()
    }
}
