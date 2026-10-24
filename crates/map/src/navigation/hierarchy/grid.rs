//! One transient dense chunk, reused for all portal floods, then discarded.
//! The retained graph holds only costs and component labels, not tile edges.
use super::*;

pub(super) const CELLS: usize = (CHUNK_TILES * CHUNK_TILES) as usize;
const OFFSETS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

pub(super) fn index(tile: TileCoord) -> usize {
    (tile.y.rem_euclid(CHUNK_TILES) * CHUNK_TILES + tile.x.rem_euclid(CHUNK_TILES)) as usize
}

pub(super) fn tile(key: Key, index: usize) -> TileCoord {
    TileCoord::new(
        key.0 * CHUNK_TILES + index as i32 % CHUNK_TILES,
        key.1 * CHUNK_TILES + index as i32 / CHUNK_TILES,
    )
}

#[derive(Debug)]
pub(super) enum Labels {
    Empty,
    Open,
    Tiles(Box<[u16; CELLS]>),
}
impl Labels {
    pub(super) fn bytes(&self) -> usize {
        match self {
            Self::Tiles(_) => std::mem::size_of::<[u16; CELLS]>(),
            Self::Empty | Self::Open => 0,
        }
    }
}
impl std::ops::Deref for Labels {
    type Target = [u16; CELLS];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Empty => &[0; CELLS],
            Self::Open => &[1; CELLS],
            Self::Tiles(labels) => labels,
        }
    }
}

pub(super) struct Grid {
    edges: Box<[[u16; 8]; CELLS]>,
    open: Box<[bool; CELLS]>,
    flat: bool,
    pub(super) uniform_height: bool,
}

impl Grid {
    pub(super) fn new(ground: &Ground<'_>, key: Key) -> Result<Self> {
        let mut grid = Self {
            edges: Box::new([[0; 8]; CELLS]),
            open: Box::new([false; CELLS]),
            flat: false,
            uniform_height: false,
        };
        let cells = ground.generator.navigation_cells(key, ground.cancelled)?;
        let height = cells
            .iter()
            .flatten()
            .next()
            .map(|cell| cell.game_height_level);
        grid.uniform_height = cells
            .iter()
            .flatten()
            .all(|cell| Some(cell.game_height_level) == height);
        for (at, cell) in cells.iter().enumerate() {
            let tile = tile(key, at);
            grid.open[at] = cell.is_some_and(|cell| {
                cell.passable
                    && !ground.overlay.blocks_tile(tile)
                    && cell
                        .node
                        .is_none_or(|node| !ground.overlay.blocks_node(node))
            });
        }
        let edge = |a: usize, b: usize| {
            let (Some(a), Some(b)) = (cells[a], cells[b]) else {
                return false;
            };
            a.passable
                && b.passable
                && a.walkable_surface
                && b.walkable_surface
                && (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() <= 1
        };
        for at in 0..CELLS {
            if (ground.cancelled)() {
                return Err(EnvironmentPageError::Cancelled);
            }
            if !grid.open[at] {
                continue;
            }
            let from = tile(key, at);
            for (direction, (dx, dy)) in OFFSETS.iter().copied().enumerate() {
                let to = TileCoord::new(from.x + dx, from.y + dy);
                if !inside(key, to) {
                    continue;
                }
                let next = index(to);
                if !grid.open[next] || !edge(at, next) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    let horizontal = index(TileCoord::new(to.x, from.y));
                    let vertical = index(TileCoord::new(from.x, to.y));
                    if !grid.open[horizontal]
                        || !grid.open[vertical]
                        || !edge(at, horizontal)
                        || !edge(at, vertical)
                    {
                        continue;
                    }
                }
                grid.edges[at][direction] =
                    step_cost(from, to, cells[next].is_some_and(|cell| cell.mud)) as u16;
            }
        }
        grid.flat = grid.open.iter().all(|open| *open)
            && (0..CELLS).all(|at| {
                let from = tile(key, at);
                OFFSETS.iter().enumerate().all(|(direction, (dx, dy))| {
                    let to = TileCoord::new(from.x + dx, from.y + dy);
                    let expected = if inside(key, to) {
                        step_cost(from, to, false) as u16
                    } else {
                        0
                    };
                    grid.edges[at][direction] == expected
                })
            });
        Ok(grid)
    }

    fn neighbor(at: usize, direction: usize) -> usize {
        let (dx, dy) = OFFSETS[direction];
        (at as i32 + dy * CHUNK_TILES + dx) as usize
    }

    pub(super) fn costs(
        &self,
        source: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Box<[u32; CELLS]>> {
        self.flood(source, cancelled, false)
    }
    pub(super) fn costs_to(
        &self,
        source: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Box<[u32; CELLS]>> {
        self.flood(source, cancelled, true)
    }
    fn flood(
        &self,
        source: TileCoord,
        cancelled: &dyn Fn() -> bool,
        reverse: bool,
    ) -> Result<Box<[u32; CELLS]>> {
        let mut best = Box::new([u32::MAX; CELLS]);
        let start = index(source);
        if !self.open[start] {
            return Ok(best);
        }
        if self.flat {
            for (at, cost) in best.iter_mut().enumerate() {
                let (x, y) = (at as i32 % CHUNK_TILES, at as i32 / CHUNK_TILES);
                let (dx, dy) = (
                    (x - source.x.rem_euclid(CHUNK_TILES)).unsigned_abs(),
                    (y - source.y.rem_euclid(CHUNK_TILES)).unsigned_abs(),
                );
                *cost = dx.min(dy) * DIAGONAL_COST + dx.abs_diff(dy) * ORTHOGONAL_COST;
            }
            return Ok(best);
        }
        best[start] = 0;
        let mut heap = BinaryHeap::from([Reverse((0_u32, start))]);
        while let Some(Reverse((cost, at))) = heap.pop() {
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            if best[at] != cost {
                continue;
            }
            for direction in 0..8 {
                let Some(next) = Self::adjacent(at, direction) else {
                    continue;
                };
                let step = if reverse {
                    self.edges[next][7 - direction]
                } else {
                    self.edges[at][direction]
                };
                if step == 0 {
                    continue;
                }
                let total = cost + u32::from(step);
                if total < best[next] {
                    best[next] = total;
                    heap.push(Reverse((total, next)));
                }
            }
        }
        Ok(best)
    }

    pub(super) fn components(&self) -> (Labels, u16, Vec<(u16, u16)>) {
        if self.flat {
            return (Labels::Open, 1, Vec::new());
        }
        // Kosaraju: directed diagonal cliff transitions must not be treated
        // as undirected land. Retain their arcs between local components.
        let mut seen = [false; CELLS];
        let mut order = Vec::new();
        let mut frames = Vec::new();
        for start in 0..CELLS {
            if !self.open[start] || seen[start] {
                continue;
            }
            seen[start] = true;
            frames.push((start, 0));
            while let Some((at, direction)) = frames.pop() {
                if direction == 8 {
                    order.push(at);
                    continue;
                }
                frames.push((at, direction + 1));
                if self.edges[at][direction] == 0 {
                    continue;
                }
                let next = Self::neighbor(at, direction);
                if !seen[next] {
                    seen[next] = true;
                    frames.push((next, 0));
                }
            }
        }
        let mut labels = Box::new([0; CELLS]);
        let mut count = 0;
        let mut stack = Vec::new();
        for start in order.into_iter().rev() {
            if labels[start] != 0 {
                continue;
            }
            count += 1;
            labels[start] = count;
            stack.push(start);
            while let Some(at) = stack.pop() {
                for direction in 0..8 {
                    let Some(next) = Self::adjacent(at, direction) else {
                        continue;
                    };
                    if self.edges[next][7 - direction] != 0 && labels[next] == 0 {
                        labels[next] = count;
                        stack.push(next);
                    }
                }
            }
        }
        let mut links = std::collections::BTreeSet::new();
        for at in 0..CELLS {
            for direction in 0..8 {
                if self.edges[at][direction] != 0 {
                    let next = Self::neighbor(at, direction);
                    if labels[at] != labels[next] {
                        links.insert((labels[at], labels[next]));
                    }
                }
            }
        }
        let labels = if count == 0 {
            Labels::Empty
        } else if count == 1 && self.open.iter().all(|open| *open) {
            Labels::Open
        } else {
            Labels::Tiles(labels)
        };
        (labels, count, links.into_iter().collect())
    }

    fn adjacent(at: usize, direction: usize) -> Option<usize> {
        let (dx, dy) = OFFSETS[direction];
        let (x, y) = (at as i32 % CHUNK_TILES + dx, at as i32 / CHUNK_TILES + dy);
        (x >= 0 && y >= 0 && x < CHUNK_TILES && y < CHUNK_TILES)
            .then_some((y * CHUNK_TILES + x) as usize)
    }
}
