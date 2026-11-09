//! Classify each step once, then label reversible links with compact arrays.
//! One-way steps and gates stay explicit graph links.
use super::{CELLS, CHUNK, Cells, Labels, RegionGround, RegionTile, Steps};
use aoe_core::TileCoord;
use std::hash::{Hash, Hasher};

pub(super) const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

pub(super) fn origin(key: (i32, i32)) -> TileCoord {
    TileCoord::new(key.0 * CHUNK, key.1 * CHUNK)
}

pub(super) fn index(tile: TileCoord) -> usize {
    (tile.y.rem_euclid(CHUNK) * CHUNK + tile.x.rem_euclid(CHUNK)) as usize
}

fn neighbour(cell: usize, direction: usize) -> Option<usize> {
    let (dx, dy) = NEIGHBOURS[direction];
    let (x, y) = (cell as i32 % CHUNK + dx, cell as i32 / CHUNK + dy);
    (x >= 0 && y >= 0 && x < CHUNK && y < CHUNK).then_some((y * CHUNK + x) as usize)
}

pub struct Input {
    pub(super) classes: Vec<RegionTile>,
    pub(super) steps: Steps,
    pub(super) signature: u64,
}

/// Capture classes and all directed edges, including the border halo.
/// Edge-only changes (cliffs/decks) participate in the fingerprint.
pub(super) fn input(ground: &dyn RegionGround, key: (i32, i32), size: (i32, i32)) -> Input {
    if let Some(input) = ground.chunk(key, size) {
        return input;
    }
    let at = origin(key);
    let mut classes = Vec::with_capacity(CELLS);
    for y in at.y..at.y + CHUNK {
        for x in at.x..at.x + CHUNK {
            classes.push(if x < size.0 && y < size.1 {
                ground.tile(TileCoord::new(x, y))
            } else {
                RegionTile::Blocked
            });
        }
    }
    let class = |tile: TileCoord| {
        if tile.x < 0 || tile.y < 0 || tile.x >= size.0 || tile.y >= size.1 {
            RegionTile::Blocked
        } else if tile.x >= at.x && tile.y >= at.y && tile.x < at.x + CHUNK && tile.y < at.y + CHUNK
        {
            classes[index(tile)]
        } else {
            ground.tile(tile)
        }
    };
    let mut masks = vec![0_u8; CELLS];
    for (cell, kind) in classes.iter().enumerate() {
        if *kind == RegionTile::Blocked {
            continue;
        }
        let tile = TileCoord::new(at.x + cell as i32 % CHUNK, at.y + cell as i32 / CHUNK);
        for (bit, (dx, dy)) in NEIGHBOURS.into_iter().enumerate() {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if class(next) != RegionTile::Blocked && step_allowed(ground, &class, tile, next) {
                masks[cell] |= 1 << bit;
            }
        }
    }
    Input::new(classes, masks)
}

impl Input {
    pub(super) fn new(classes: Vec<RegionTile>, masks: Vec<u8>) -> Self {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        classes.hash(&mut hash);
        masks.hash(&mut hash);
        Self {
            classes,
            steps: Cells::compact(masks),
            signature: hash.finish(),
        }
    }
}

/// The planner's corner rule. Gate permission belongs to graph traversal.
pub(super) fn step_allowed(
    ground: &dyn RegionGround,
    class: &dyn Fn(TileCoord) -> RegionTile,
    from: TileCoord,
    to: TileCoord,
) -> bool {
    if from.x != to.x && from.y != to.y {
        if let Some(clear) = ground.diagonal_override(from, to) {
            return clear;
        }
        let horizontal = TileCoord::new(to.x, from.y);
        let vertical = TileCoord::new(from.x, to.y);
        if class(horizontal) == RegionTile::Blocked
            || class(vertical) == RegionTile::Blocked
            || !ground.edge_clear(from, horizontal)
            || !ground.edge_clear(from, vertical)
        {
            return false;
        }
    }
    ground.edge_clear(from, to)
}

pub(super) struct Labelled {
    pub labels: Labels,
    pub gates: Vec<Option<u32>>,
    pub local: Vec<(u16, u16, bool)>,
}

/// Row-order labels over reversible same-class edges. Uniform open chunks
/// keep implicit labels and step masks, rather than per-tile allocations.
pub(super) fn label(input: &Input) -> Labelled {
    let classes = &input.classes;
    if classes.iter().all(|&class| class == RegionTile::Blocked) {
        return Labelled {
            labels: Cells::Uniform(0),
            gates: Vec::new(),
            local: Vec::new(),
        };
    }
    if matches!(input.steps, Steps::Uniform(u8::MAX)) && classes.iter().all(|&c| c == classes[0]) {
        return Labelled {
            labels: Cells::Uniform(1),
            gates: vec![gate(classes[0])],
            local: Vec::new(),
        };
    }
    let mut labels = vec![0_u16; CELLS];
    let mut gates = Vec::new();
    let mut stack = Vec::new();
    for start in 0..CELLS {
        if labels[start] != 0 || classes[start] == RegionTile::Blocked {
            continue;
        }
        gates.push(gate(classes[start]));
        let region = gates.len() as u16;
        labels[start] = region;
        stack.push(start);
        while let Some(cell) = stack.pop() {
            for direction in 0..8 {
                let Some(next) = neighbour(cell, direction) else {
                    continue;
                };
                if labels[next] == 0
                    && classes[next] == classes[start]
                    && input.steps.at(cell) & (1 << direction) != 0
                    && input.steps.at(next) & (1 << (7 - direction)) != 0
                {
                    labels[next] = region;
                    stack.push(next);
                }
            }
        }
    }
    let mut local = Vec::new();
    for cell in 0..CELLS {
        if labels[cell] == 0 {
            continue;
        }
        // E, SW, S and SE own each unordered pair exactly once.
        for direction in [4, 5, 6, 7] {
            let Some(next) = neighbour(cell, direction) else {
                continue;
            };
            let (a, b) = (labels[cell], labels[next]);
            if b == 0 || a == b {
                continue;
            }
            let forward = input.steps.at(cell) & (1 << direction) != 0;
            let reverse = input.steps.at(next) & (1 << (7 - direction)) != 0;
            if forward {
                local.push((a, b, reverse));
            } else if reverse {
                local.push((b, a, false));
            }
        }
    }
    local.sort_unstable();
    local.dedup();
    Labelled {
        labels: Cells::compact(labels),
        gates,
        local,
    }
}

fn gate(class: RegionTile) -> Option<u32> {
    match class {
        RegionTile::Gate(gate) => Some(gate),
        _ => None,
    }
}
