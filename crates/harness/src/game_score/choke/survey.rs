//! Finding chokes in the ground itself, from nothing but which tiles a unit can walk on.
//!
//! A choke is where two large stretches of ground are joined only by a neck. The survey finds
//! the necks the way a watershed finds its passes. Every walkable tile gets its clearance (the
//! distance to the nearest tile that blocks), the tiles are taken from the widest to the
//! narrowest, and each joins the stretch of ground around it. When a tile joins two stretches that
//! are each large, that tile is the narrowest point of the widest way between them: a saddle. Its
//! clearance says how wide the neck is, and the size of the two stretches says how much ground
//! the neck separates. `corridor` then measures the neck exactly: the cross-section, its width and
//! how far it runs.
use super::Tile;

/// What the survey needs to know of a map: which tiles a unit can walk on.
pub(in crate::game_score) trait Ground: Sync {
    /// Tiles on a side: the map is a square, `0..side` each way.
    fn side(&self) -> i32;
    fn walkable(&self, tile: Tile) -> bool;
    /// One character for what is on `tile`, for a picture of a neck.
    fn glyph(&self, tile: Tile) -> char {
        if self.walkable(tile) { '.' } else { '#' }
    }
}

/// Which tiles of a square map are walkable, for the whole map at once.
pub(in crate::game_score) struct Grid {
    side: i32,
    ground: Vec<bool>,
}

impl Grid {
    pub fn of(ground: &dyn Ground) -> Self {
        use rayon::prelude::*;
        let side = ground.side();
        let ground: Vec<bool> = (0..side * side)
            .into_par_iter()
            .map(|at| ground.walkable((at % side, at / side)))
            .collect();
        Self { side, ground }
    }

    pub fn side(&self) -> i32 {
        self.side
    }

    pub fn walkable(&self, (x, y): Tile) -> bool {
        (0..self.side).contains(&x)
            && (0..self.side).contains(&y)
            && self.ground[(y * self.side + x) as usize]
    }

    fn index(&self, (x, y): Tile) -> usize {
        (y * self.side + x) as usize
    }
}

/// Steps of the clearance: a tile next to a blocked one has 3, one tile further has 6, a
/// diagonal step costs 4 (the chamfer 3-4 approximation of a distance in thirds of a tile).
pub(in crate::game_score) const THIRDS: u16 = 3;

/// For every tile, the distance to the nearest tile that blocks (or the map's edge), in thirds of
/// a tile; 0 for a tile that blocks.
pub(in crate::game_score) fn clearance(grid: &Grid) -> Vec<u16> {
    let side = grid.side;
    let far = u16::MAX / 2;
    let mut distance: Vec<u16> = (0..side * side)
        .map(|at| {
            if grid.walkable((at % side, at / side)) {
                far
            } else {
                0
            }
        })
        .collect();
    let at = |x: i32, y: i32| (y * side + x) as usize;
    let toward = |distance: &mut [u16], x: i32, y: i32, from: &[(i32, i32, u16)]| {
        let mut best = distance[at(x, y)];
        for &(dx, dy, cost) in from {
            let (nx, ny) = (x + dx, y + dy);
            let reach = if (0..side).contains(&nx) && (0..side).contains(&ny) {
                distance[at(nx, ny)].saturating_add(cost)
            } else {
                // The map's edge blocks: the tile beside it is one step from a blocked tile.
                cost
            };
            best = best.min(reach);
        }
        distance[at(x, y)] = best;
    };
    let forward = [(-1, 0, 3), (0, -1, 3), (-1, -1, 4), (1, -1, 4)];
    let backward = [(1, 0, 3), (0, 1, 3), (1, 1, 4), (-1, 1, 4)];
    for y in 0..side {
        for x in 0..side {
            toward(&mut distance, x, y, &forward);
        }
    }
    for y in (0..side).rev() {
        for x in (0..side).rev() {
            toward(&mut distance, x, y, &backward);
        }
    }
    distance
}

/// A neck between two large stretches of ground.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::game_score) struct Saddle {
    /// The narrowest point of the widest way between the two stretches.
    pub tile: Tile,
    /// Its clearance in thirds of a tile.
    pub clearance: u16,
    /// Tiles of ground (wider than the neck) on each side of it, the smaller first.
    pub sides: (usize, usize),
}

struct Sets {
    parent: Vec<u32>,
    size: Vec<u32>,
}

impl Sets {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len as u32).collect(),
            size: vec![1; len],
        }
    }

    fn root(&mut self, mut tile: u32) -> u32 {
        while self.parent[tile as usize] != tile {
            let up = self.parent[self.parent[tile as usize] as usize];
            self.parent[tile as usize] = up;
            tile = up;
        }
        tile
    }

    /// Joins the sets of `a` and `b`; their sizes before, or `None` if they were one.
    fn join(&mut self, a: u32, b: u32) -> Option<(u32, u32)> {
        let (a, b) = (self.root(a), self.root(b));
        if a == b {
            return None;
        }
        let sizes = (self.size[a as usize], self.size[b as usize]);
        let (big, small) = if sizes.0 >= sizes.1 { (a, b) } else { (b, a) };
        self.parent[small as usize] = big;
        self.size[big as usize] = sizes.0 + sizes.1;
        Some(sizes)
    }
}

/// The saddles whose clearance is at most `widest` thirds of a tile and that join two stretches
/// of at least `min_side` tiles each, the narrowest first and, among equals, the one that
/// separates the most ground. Ground is joined through its four neighbours: a gap that only
/// touches at a corner is no way through for an army.
pub(in crate::game_score) fn saddles(
    grid: &Grid,
    clearance: &[u16],
    widest: u16,
    min_side: usize,
) -> Vec<Saddle> {
    let side = grid.side;
    let order = {
        let mut tiles: Vec<u32> = (0..clearance.len() as u32)
            .filter(|&at| clearance[at as usize] > 0)
            .collect();
        // Widest first; the tile number breaks ties so the result never depends on the sort.
        tiles.sort_unstable_by_key(|&at| (std::cmp::Reverse(clearance[at as usize]), at));
        tiles
    };
    let mut sets = Sets::new(clearance.len());
    let mut active = vec![false; clearance.len()];
    let mut found = Vec::new();
    for at in order {
        active[at as usize] = true;
        let (x, y) = (at as i32 % side, at as i32 / side);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !(0..side).contains(&nx) || !(0..side).contains(&ny) {
                continue;
            }
            let next = (ny * side + nx) as u32;
            if !active[next as usize] {
                continue;
            }
            let Some((a, b)) = sets.join(at, next) else {
                continue;
            };
            let (small, big) = (a.min(b) as usize, a.max(b) as usize);
            if small >= min_side && clearance[at as usize] <= widest {
                found.push(Saddle {
                    tile: (x, y),
                    clearance: clearance[at as usize],
                    sides: (small, big),
                });
            }
        }
    }
    found.sort_by_key(|saddle| {
        (
            saddle.clearance,
            std::cmp::Reverse(saddle.sides.0),
            saddle.tile,
        )
    });
    found
}

/// A neck measured: its cross-section and how far it runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::game_score) struct Corridor {
    /// The tile that blocks at each end of the cross-section, west or north first.
    pub ends: (Tile, Tile),
    /// Walkable tiles between them.
    pub width: u32,
    /// Along the neck: the cross-sections in a row (both ways from the first) that are no wider
    /// than the narrowest one plus `SLACK`, the narrowest included.
    pub length: u32,
    /// The direction of the march through the neck: a unit step, one of `(1, 0)` `(0, 1)`.
    pub flow: Tile,
}

/// The tiles between the two ends of a cross-section, ends excluded, in order.
pub(in crate::game_score) fn across(ends: (Tile, Tile)) -> Vec<Tile> {
    let (a, b) = ends;
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs());
    let step = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
    (1..steps)
        .map(|n| (a.0 + step.0 * n, a.1 + step.1 * n))
        .collect()
}

/// A cross-section may be this many tiles wider than the narrowest and still be part of the neck.
pub(in crate::game_score) const SLACK: u32 = 2;

/// The run of walkable tiles through `tile` along `step`, and the blocked tile that ends it each
/// way. `None` when `tile` itself blocks, or no end is found within `limit` tiles.
fn run(
    walkable: &dyn Fn(Tile) -> bool,
    tile: Tile,
    step: Tile,
    limit: i32,
) -> Option<(Tile, Tile, u32)> {
    if !walkable(tile) {
        return None;
    }
    let walk = |sign: i32| {
        (1..=limit).find_map(|n| {
            let at = (tile.0 + sign * step.0 * n, tile.1 + sign * step.1 * n);
            (!walkable(at)).then_some((at, n as u32 - 1))
        })
    };
    let (before, back) = walk(-1)?;
    let (after, ahead) = walk(1)?;
    Some((before, after, back + ahead + 1))
}

/// The cross-section of the neck around `saddle`: the narrower of the horizontal and the vertical
/// run through it, then how far the neck runs along the march. `None` if the ground around is
/// open (no end within 64 tiles).
pub(in crate::game_score) fn corridor(
    walkable: &dyn Fn(Tile) -> bool,
    saddle: Tile,
) -> Option<Corridor> {
    let horizontal = run(walkable, saddle, (1, 0), 64);
    let vertical = run(walkable, saddle, (0, 1), 64);
    let (across, flow) = match (horizontal, vertical) {
        (Some(h), Some(v)) if h.2 <= v.2 => (h, (0, 1)),
        (Some(_), Some(v)) | (None, Some(v)) => (v, (1, 0)),
        (Some(h), None) => (h, (0, 1)),
        (None, None) => return None,
    };
    let along = (flow.1, flow.0);
    let width = across.2;
    let mut length = 1;
    for sign in [-1, 1] {
        let mut centre = saddle;
        for _ in 0..256 {
            centre = (centre.0 + sign * flow.0, centre.1 + sign * flow.1);
            let (a, b, w) = match run(walkable, centre, along, 64) {
                Some(measured) => measured,
                // Open ground: the neck has ended.
                None if walkable(centre) => break,
                // The centre fell on a wall: look for ground one tile either way.
                None => {
                    let nudged = [-1, 1]
                        .into_iter()
                        .map(|n| (centre.0 + n * along.0, centre.1 + n * along.1))
                        .find(|&tile| walkable(tile));
                    match nudged {
                        Some(tile) => {
                            centre = tile;
                            continue;
                        }
                        None => break,
                    }
                }
            };
            if w > width + SLACK {
                break;
            }
            // Re-centre on the run so a corridor that bends is followed.
            centre = ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
            length += 1;
        }
    }
    Some(Corridor {
        ends: (across.0, across.1),
        width,
        length,
        flow,
    })
}

/// Open ground (clearance of at least two tiles) as near as possible to `wanted`, searched in
/// rings out to `reach` tiles.
fn open_near(grid: &Grid, clearance: &[u16], wanted: Tile, reach: i32) -> Option<Tile> {
    (0..=reach).find_map(|ring| {
        let mut best: Option<Tile> = None;
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let tile = (wanted.0 + x, wanted.1 + y);
                if grid.walkable(tile)
                    && clearance[grid.index(tile)] >= 2 * THIRDS
                    && best.is_none_or(|b| tile < b)
                {
                    best = Some(tile);
                }
            }
        }
        best
    })
}

/// Where two armies stand on either side of a neck: `distance` tiles from the middle of its
/// cross-section along the march, on open ground.
pub(in crate::game_score) fn anchors(
    grid: &Grid,
    clearance: &[u16],
    corridor: &Corridor,
    distance: i32,
) -> Option<(Tile, Tile)> {
    let (a, b) = corridor.ends;
    let middle = ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
    let at = |sign: i32| {
        open_near(
            grid,
            clearance,
            (
                middle.0 + sign * distance * corridor.flow.0,
                middle.1 + sign * distance * corridor.flow.1,
            ),
            12,
        )
    };
    Some((at(-1)?, at(1)?))
}

/// The walking distance in tiles (steps through the four neighbours) from `from` to `to`, not
/// stepping on any of `closed`; `None` when there is no way.
pub(in crate::game_score) fn steps(
    grid: &Grid,
    from: Tile,
    to: Tile,
    closed: &[Tile],
) -> Option<u32> {
    let side = grid.side;
    let mut seen = vec![false; (side * side) as usize];
    for &tile in closed {
        seen[grid.index(tile)] = true;
    }
    let mut frontier = vec![from];
    seen[grid.index(from)] = true;
    let mut distance = 0;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for (x, y) in frontier {
            if (x, y) == to {
                return Some(distance);
            }
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let tile = (x + dx, y + dy);
                if grid.walkable(tile) && !seen[grid.index(tile)] {
                    seen[grid.index(tile)] = true;
                    next.push(tile);
                }
            }
        }
        frontier = next;
        distance += 1;
    }
    None
}

/// What blocks the two sides of a neck: the glyph of the blocked tile at each end of the
/// cross-section and of every cross-section along the neck, counted.
pub(in crate::game_score) fn flanks(ground: &dyn Ground, corridor: &Corridor) -> String {
    let mut counts: std::collections::BTreeMap<char, u32> = std::collections::BTreeMap::new();
    let reach = corridor.length as i32 / 2 + 1;
    for step in -reach..=reach {
        for end in [corridor.ends.0, corridor.ends.1] {
            let tile = (
                end.0 + step * corridor.flow.0,
                end.1 + step * corridor.flow.1,
            );
            if !ground.walkable(tile) {
                *counts.entry(ground.glyph(tile)).or_default() += 1;
            }
        }
    }
    counts
        .iter()
        .map(|(glyph, count)| format!("{glyph}x{count}"))
        .collect::<Vec<_>>()
        .join(" ")
}
