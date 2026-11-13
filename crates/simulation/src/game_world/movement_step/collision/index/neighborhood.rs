//! Borrowed cell samples live for one sidestep decision, before any relocation.
//! Cache misses use the live index. Cell/body order and overflow policy remain
//! the same for every candidate, including queries outside the cached square.
use super::{BodyBox, CellBody, CollisionIndex};
use crate::game_world::route_scheduler::GroupWork;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::cell::Cell;

const SIDE: usize = 5;

#[derive(Clone, Copy)]
pub(super) struct Sample<'a> {
    slices: [&'a [CellBody]; 2],
    overfull: bool,
}

impl<'a> Sample<'a> {
    pub(super) fn new(bodies: &'a [CellBody], salt: u64, count: usize, overfull: bool) -> Self {
        let len = bodies.len();
        debug_assert!(count <= len);
        let start = if len <= 1 {
            0
        } else {
            (salt % len as u64) as usize
        };
        let end = (start + count).min(len);
        Self {
            slices: [&bodies[start..end], &bodies[..count - (end - start)]],
            overfull,
        }
    }
}

pub(crate) struct Neighborhood<'a> {
    index: &'a CollisionIndex,
    origin: TileCoord,
    naval: bool,
    salt: u64,
    loaded: Cell<u32>,
    samples: [Cell<Option<Sample<'a>>>; SIDE * SIDE],
}

impl<'a> Neighborhood<'a> {
    pub(super) fn new(
        index: &'a CollisionIndex,
        centre: TileCoord,
        naval: bool,
        salt: u64,
    ) -> Self {
        Self {
            index,
            origin: TileCoord::new(centre.x.saturating_sub(2), centre.y.saturating_sub(2)),
            naval,
            salt,
            loaded: Cell::new(0),
            samples: std::array::from_fn(|_| Cell::new(None)),
        }
    }

    fn sample(&self, tile: TileCoord) -> Option<Sample<'a>> {
        let x = i64::from(tile.x) - i64::from(self.origin.x);
        let y = i64::from(tile.y) - i64::from(self.origin.y);
        if !(0..SIDE as i64).contains(&x) || !(0..SIDE as i64).contains(&y) {
            return self.index.sample(tile, self.naval, self.salt);
        }
        let at = y as usize * SIDE + x as usize;
        let bit = 1 << at;
        if self.loaded.get() & bit == 0 {
            self.samples[at].set(self.index.sample(tile, self.naval, self.salt));
            self.loaded.set(self.loaded.get() | bit);
        }
        self.samples[at].get()
    }

    pub(crate) fn blocked_in(
        &self,
        query: (WorldPosition, i32),
        window: BodyBox,
        work: &GroupWork,
        test: impl FnMut(EntityId) -> bool,
        overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        blocked_cells(query, window, work, test, overflow, |tile| {
            self.sample(tile)
        })
    }
}

#[inline]
pub(super) fn blocked_cells<'a>(
    (at, reach): (WorldPosition, i32),
    window: BodyBox,
    work: &GroupWork,
    mut test: impl FnMut(EntityId) -> bool,
    mut overflow: impl FnMut(TileCoord) -> bool,
    mut sample: impl FnMut(TileCoord) -> Option<Sample<'a>>,
) -> bool {
    let min =
        WorldPosition::new(at.x.saturating_sub(reach), at.y.saturating_sub(reach)).tile_floor();
    let max =
        WorldPosition::new(at.x.saturating_add(reach), at.y.saturating_add(reach)).tile_floor();
    let mut probes = 0;
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            probes += 1;
            let tile = TileCoord::new(x, y);
            let Some(cell) = sample(tile) else {
                continue;
            };
            if cell.overfull && overflow(tile) {
                work.collision(probes);
                return true;
            }
            for bodies in cell.slices {
                for body in bodies {
                    probes += 1;
                    if body.at.x >= window.0
                        && body.at.x <= window.1
                        && body.at.y >= window.2
                        && body.at.y <= window.3
                        && test(body.id)
                    {
                        work.collision(probes);
                        return true;
                    }
                }
            }
        }
    }
    work.collision(probes);
    false
}
