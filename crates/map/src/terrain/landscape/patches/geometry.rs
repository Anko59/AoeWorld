//! Warped coordinate space, trail nodes and trails of the forest descriptor.
use super::{CELL_TILES, Patches};

impl Patches {
    /// Warped coordinates of a tile: the space of every stand, glade and trail.
    pub fn warped(&self, x: i64, y: i64) -> (i64, i64) {
        (
            x + self.warp(5, 64, 12, x, y) + self.warp(6, 20, 3, x, y),
            y + self.warp(7, 64, 12, x, y) + self.warp(8, 20, 3, x, y),
        )
    }
    /// Trail node of a cell, in warped coordinates: within 40 tiles of the cell
    /// centre. The hub glade around it is cleared only inside a large forest.
    pub fn node(&self, cx: i64, cy: i64) -> (i64, i64) {
        let h = self.noise(1, cx, cy);
        (
            cx * CELL_TILES + CELL_TILES / 2 + (h % 81) as i64 - 40,
            cy * CELL_TILES + CELL_TILES / 2 + ((h >> 8) % 81) as i64 - 40,
        )
    }
    /// Tile whose warped coordinates are nearest the cell's trail node: a
    /// bounded 33x33 search, meant for route probes rather than per-tile use.
    pub fn node_tile(&self, cx: i64, cy: i64) -> (i64, i64) {
        let node = self.node(cx, cy);
        let mut best = (i64::MAX, node);
        for y in node.1 - 16..=node.1 + 16 {
            for x in node.0 - 16..=node.0 + 16 {
                let warped = self.warped(x, y);
                let distance = (warped.0 - node.0).pow(2) + (warped.1 - node.1).pow(2);
                if distance < best.0 {
                    best = (distance, (x, y));
                }
            }
        }
        best.1
    }
    /// Hub glades and the bent trails joining adjacent nodes, for a point in
    /// warped coordinates. Membership of the large-forest domain is decided
    /// by the caller.
    pub fn trail(&self, tx: i64, ty: i64) -> bool {
        let (cx, cy) = (tx.div_euclid(CELL_TILES), ty.div_euclid(CELL_TILES));
        let node = self.node(cx, cy);
        let hub = 8 + ((self.noise(12, cx, cy) >> 20) % 4) as i64;
        if (tx - node.0).pow(2) + (ty - node.1).pow(2) <= hub * hub {
            return true;
        }
        [
            ((cx, cy), true),
            ((cx - 1, cy), true),
            ((cx, cy), false),
            ((cx, cy - 1), false),
        ]
        .into_iter()
        .any(|((sx, sy), east)| {
            let start = self.node(sx, sy);
            let end = if east {
                self.node(sx + 1, sy)
            } else {
                self.node(sx, sy + 1)
            };
            let h = self.noise(if east { 13 } else { 14 }, sx, sy);
            let offset = (7 + ((h >> 19) % 13) as i64) * if h & 1 == 0 { -1 } else { 1 };
            let middle = (
                (start.0 + end.0).div_euclid(2),
                (start.1 + end.1).div_euclid(2),
            );
            let bend = if east {
                (middle.0, middle.1 + offset)
            } else {
                (middle.0 + offset, middle.1)
            };
            near_segment(start, bend, (tx, ty), 2) || near_segment(bend, end, (tx, ty), 2)
        })
    }
    // Bilinear integer warp. Euclidean division makes signed world-coordinate
    // seams and chunk halos identical.
    pub(super) fn warp(&self, domain: u64, lattice: i64, amplitude: i64, x: i64, y: i64) -> i64 {
        let (cx, cy) = (x.div_euclid(lattice), y.div_euclid(lattice));
        let (ox, oy) = (x.rem_euclid(lattice), y.rem_euclid(lattice));
        let node = |dx, dy| (self.noise(domain, cx + dx, cy + dy) % 2049) as i64 - 1024;
        let north = node(0, 0) * (lattice - ox) + node(1, 0) * ox;
        let south = node(0, 1) * (lattice - ox) + node(1, 1) * ox;
        ((north * (lattice - oy) + south * oy) * amplitude).div_euclid(1024 * lattice * lattice)
    }
}

pub(super) fn near_segment(
    start: (i64, i64),
    end: (i64, i64),
    point: (i64, i64),
    radius: i64,
) -> bool {
    let (dx, dy) = (i128::from(end.0 - start.0), i128::from(end.1 - start.1));
    let (px, py) = (i128::from(point.0 - start.0), i128::from(point.1 - start.1));
    let (length, projection) = (dx * dx + dy * dy, px * dx + py * dy);
    let limit = i128::from(radius * radius);
    if projection <= 0 {
        return px * px + py * py <= limit;
    }
    if projection >= length {
        let (ex, ey) = (i128::from(point.0 - end.0), i128::from(point.1 - end.1));
        return ex * ex + ey * ey <= limit;
    }
    let cross = px * dy - py * dx;
    cross * cross <= limit * length
}
