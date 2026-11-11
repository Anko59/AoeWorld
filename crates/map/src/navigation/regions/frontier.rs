//! Frontier goals: the reachable tile nearest to a target, found by visiting
//! chunks in order of their distance to the target. Gate tiles are never a
//! frontier (nobody should park in a gateway).
use super::{CHUNK, RegionGround, Regions, label};
use aoe_core::TileCoord;
use std::collections::BTreeSet;

fn squared(a: TileCoord, b: TileCoord) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}

impl Regions {
    pub(super) fn chunk_distance(&self, chunk: u32, target: TileCoord) -> i64 {
        let at = label::origin(self.key(chunk));
        let near = TileCoord::new(
            target.x.clamp(at.x, at.x + CHUNK - 1),
            target.y.clamp(at.y, at.y + CHUNK - 1),
        );
        squared(near, target)
    }

    /// The tile `player`, standing on `from`, can reach that is nearest to
    /// `target` (ties: nearer to `from`, then lower y, then lower x). `target` itself when it is
    /// reachable; `None` when `from` is not passable.
    pub fn frontier(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        target: TileCoord,
    ) -> Option<TileCoord> {
        if self.chunks.is_empty() {
            return if self.clear_ray(ground, player, from, target) {
                Some(target)
            } else {
                self.lazy_frontier(ground, player, from, target)
            };
        }
        self.flush(ground);
        let roots = self.reachable_roots(player, from);
        if roots.is_empty() {
            return None;
        }
        if let Some(goal) = self.node(target) {
            let root = self.global.root(goal);
            if roots.contains(&self.closure_root(player, root)) {
                return Some(target);
            }
        }
        let clamped = TileCoord::new(
            target.x.clamp(0, self.size.0 - 1),
            target.y.clamp(0, self.size.1 - 1),
        );
        let first = self.chunk_of(clamped)?;
        let mut heap = BTreeSet::from([(self.chunk_distance(first, target), first)]);
        let mut seen = BTreeSet::from([first]);
        let mut best: Option<(i64, i64, i32, i32)> = None;
        while let Some((distance, chunk)) = heap.pop_first() {
            if best.is_some_and(|(d, ..)| distance > d) {
                break;
            }
            self.scan(player, chunk, (from, target), &roots, &mut best);
            let (cx, cy) = self.key(chunk);
            for (dx, dy) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.chunks_x || y >= self.chunks_y {
                    continue;
                }
                let next = (y * self.chunks_x + x) as u32;
                if seen.insert(next) {
                    heap.insert((self.chunk_distance(next, target), next));
                }
            }
        }
        best.map(|(_, _, y, x)| TileCoord::new(x, y))
    }

    fn scan(
        &mut self,
        player: u32,
        chunk: u32,
        (from, target): (TileCoord, TileCoord),
        roots: &BTreeSet<u32>,
        best: &mut Option<(i64, i64, i32, i32)>,
    ) {
        let count = self.chunks[chunk as usize].gates.len();
        let mut wanted = vec![false; count + 1];
        let mut any = false;
        for label in 1..=count as u16 {
            if self.chunks[chunk as usize].gates[usize::from(label) - 1].is_some() {
                continue;
            }
            let root = self.global.root(self.node_of(chunk, label));
            if roots.contains(&self.closure_root(player, root)) {
                wanted[usize::from(label)] = true;
                any = true;
            }
        }
        if !any {
            return;
        }
        let at = label::origin(self.key(chunk));
        let labels = &self.chunks[chunk as usize].labels;
        for cell in 0..super::CELLS {
            if !wanted[usize::from(labels.at(cell))] {
                continue;
            }
            let tile = TileCoord::new(at.x + cell as i32 % CHUNK, at.y + cell as i32 / CHUNK);
            let key = (squared(tile, target), squared(tile, from), tile.y, tile.x);
            if best.is_none_or(|b| key < b) {
                *best = Some(key);
            }
        }
    }
}
