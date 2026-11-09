//! Regions and reachability: a pure layer over today's passability.
//!
//! Every 32 x 32 chunk carries `u16` region labels for its land radius bucket.
//! Two-bit directed edge tags record the minimum clearance class. Today's
//! passability admits all three buckets through a 1-tile gap. Regions are joined by a
//! two-level union-find, chunks within 8 x 8 super-chunks and then
//! super-chunks, with canonical minimum-key roots, so an answer depends on
//! the ground alone, never on the order of changes.
//!
//! Gates are gated portals: a gate's tiles form their own regions, joined to
//! the ground beside them only for the players the gate admits. The open
//! state is not part of reachability (the hard collision keeps it), so
//! toggling a gate changes no label. Changed chunks are batched and relabelled
//! on the next query; a chunk whose tile classes did not change keeps
//! everything.
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

mod bucket;
mod frontier;
use bucket::BucketEdges;
pub use bucket::LandBucket;
mod label;
mod ray;
mod terrain;
pub use label::Input as RegionChunk;
pub use terrain::MapRegionGround;
mod union;
use union::{Global, SuperRegions};

const CHUNK: i32 = 32;
const CELLS: usize = (CHUNK * CHUNK) as usize;
/// Chunks per super-chunk side.
const SUPER: i32 = 8;

/// What a tile is for land reachability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RegionTile {
    Blocked,
    Open,
    /// A completed gate's tile, by gate id.
    Gate(u32),
}

/// The passability regions read. Answers must be a pure function of the
/// current ground.
pub trait RegionGround {
    /// Optional bulk capture of the same tiles and directed edges.
    fn chunk(&self, _key: (i32, i32), _size: (i32, i32)) -> Option<RegionChunk> {
        None
    }
    fn tile(&self, tile: TileCoord) -> RegionTile;
    /// Height edges and bridge decks between two 8-neighbour tiles.
    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool;
    /// Clearance of an edge. Today's land predicate admits every radius;
    /// adapters with narrower clearance return its largest admitted bucket.
    fn edge_bucket(&self, from: TileCoord, to: TileCoord) -> Option<LandBucket> {
        self.edge_clear(from, to).then_some(LandBucket::Large)
    }
    /// Analytic decks can carry a diagonal over blocked orthogonal corners.
    fn diagonal_override(&self, _from: TileCoord, _to: TileCoord) -> Option<bool> {
        None
    }
    /// The one player a gate admits (today's owner-only rule).
    fn gate_owner(&self, gate: u32) -> u32;
}

/// A player every gate admits: plain connectivity, as the route planner
/// (which treats a completed gate as open ground) sees it.
pub const ANY_PLAYER: u32 = u32::MAX;

/// Work counters; `relabels` counts chunks whose classes changed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegionStats {
    pub builds: u64,
    pub relabels: u64,
    pub unchanged: u64,
    pub super_rebuilds: u64,
    pub global_rebuilds: u64,
}

#[derive(Debug)]
enum Cells<T> {
    Uniform(T),
    Tiles(Box<[T]>),
}
type Labels = Cells<u16>;
type Steps = Cells<BucketEdges>;
impl<T: Copy + Eq> Cells<T> {
    fn compact(cells: Vec<T>) -> Self {
        if cells.iter().all(|&cell| cell == cells[0]) {
            Self::Uniform(cells[0])
        } else {
            Self::Tiles(cells.into_boxed_slice())
        }
    }
    fn at(&self, cell: usize) -> T {
        match self {
            Self::Uniform(label) => *label,
            Self::Tiles(labels) => labels[cell],
        }
    }
}

#[derive(Debug)]
struct ChunkRegions {
    signature: u64,
    labels: Labels,
    steps: Steps,
    gates: Vec<Option<u32>>,
    local: Vec<(u16, u16, bool)>,
    /// Outgoing links: (own label, other chunk, other label, reversible).
    links: Vec<(u16, u32, u16, bool)>,
    /// Super-chunk-local component of each region (label - 1).
    comp: Vec<u16>,
}

#[derive(Debug)]
pub struct Regions {
    bucket: LandBucket,
    size: (i32, i32),
    chunks_x: i32,
    chunks_y: i32,
    chunks: Vec<ChunkRegions>,
    dirty: BTreeSet<u32>,
    supers: Vec<SuperRegions>,
    global: Global,
    gate_owner: BTreeMap<u32, u32>,
    /// Per player: gated edges admitted, folded into a small union-find.
    closures: BTreeMap<u32, BTreeMap<u32, u32>>,
    stats: RegionStats,
}

impl Regions {
    /// An empty layer; the first query labels the whole map.
    pub fn new(width: i32, height: i32, bucket: LandBucket) -> Self {
        let chunks_x = (width.max(1) + CHUNK - 1) / CHUNK;
        let chunks_y = (height.max(1) + CHUNK - 1) / CHUNK;
        Self {
            bucket,
            size: (width.max(1), height.max(1)),
            chunks_x,
            chunks_y,
            chunks: Vec::new(),
            dirty: BTreeSet::new(),
            supers: Vec::new(),
            global: Global::default(),
            gate_owner: BTreeMap::new(),
            closures: BTreeMap::new(),
            stats: RegionStats::default(),
        }
    }

    pub fn stats(&self) -> RegionStats {
        self.stats
    }

    /// Ground in `area` may have changed (tile-exclusive maximum).
    pub fn invalidate(&mut self, min: TileCoord, max: TileCoord) {
        if self.chunks.is_empty() {
            return;
        }
        let x0 = (min.x - 1).max(0).div_euclid(CHUNK);
        let y0 = (min.y - 1).max(0).div_euclid(CHUNK);
        let x1 = (max.x).min(self.size.0 - 1).div_euclid(CHUNK);
        let y1 = (max.y).min(self.size.1 - 1).div_euclid(CHUNK);
        for cy in y0..=y1.min(self.chunks_y - 1) {
            for cx in x0..=x1.min(self.chunks_x - 1) {
                self.dirty.insert((cy * self.chunks_x + cx) as u32);
            }
        }
    }

    /// Everything may have changed.
    pub fn invalidate_all(&mut self) {
        self.dirty.extend(0..self.chunks.len() as u32);
    }

    fn key(&self, chunk: u32) -> (i32, i32) {
        (chunk as i32 % self.chunks_x, chunk as i32 / self.chunks_x)
    }

    fn chunk_of(&self, tile: TileCoord) -> Option<u32> {
        (tile.x >= 0 && tile.y >= 0 && tile.x < self.size.0 && tile.y < self.size.1)
            .then(|| ((tile.y / CHUNK) * self.chunks_x + tile.x / CHUNK) as u32)
    }

    fn super_of(&self, chunk: u32) -> u32 {
        let (cx, cy) = self.key(chunk);
        let supers_x = (self.chunks_x + SUPER - 1) / SUPER;
        ((cy / SUPER) * supers_x + cx / SUPER) as u32
    }

    fn region_at(&self, tile: TileCoord) -> Option<(u32, u16)> {
        let chunk = self.chunk_of(tile)?;
        let label = self.chunks[chunk as usize].labels.at(label::index(tile));
        (label != 0).then_some((chunk, label))
    }

    /// Apply batched changes: relabel changed chunks, then rebuild only the
    /// union-find levels they touch.
    pub fn flush(&mut self, ground: &dyn RegionGround) {
        if self.chunks.is_empty() {
            self.build(ground);
            return;
        }
        let dirty = std::mem::take(&mut self.dirty);
        let mut relabelled = BTreeSet::new();
        for chunk in dirty {
            let key = self.key(chunk);
            let input = label::input(ground, key, self.size);
            self.refresh_owners(ground, &input.classes);
            if input.signature == self.chunks[chunk as usize].signature {
                self.stats.unchanged += 1;
                continue;
            }
            self.stats.relabels += 1;
            self.chunks[chunk as usize] = self.labelled(ground, input);
            relabelled.insert(chunk);
        }
        if relabelled.is_empty() {
            return;
        }
        let mut linked = BTreeSet::new();
        for &chunk in &relabelled {
            let (cx, cy) = self.key(chunk);
            for (dx, dy) in [
                (0, 0),
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
                if x >= 0 && y >= 0 && x < self.chunks_x && y < self.chunks_y {
                    linked.insert((y * self.chunks_x + x) as u32);
                }
            }
        }
        let mut supers = BTreeSet::new();
        for chunk in linked {
            self.link(chunk);
            supers.insert(self.super_of(chunk));
        }
        for at in supers {
            self.rebuild_super(at);
        }
        self.rebuild_global();
    }

    fn refresh_owners(&mut self, ground: &dyn RegionGround, classes: &[RegionTile]) {
        let gates: BTreeSet<_> = classes
            .iter()
            .filter_map(|class| match class {
                RegionTile::Gate(gate) => Some(*gate),
                _ => None,
            })
            .collect();
        for gate in gates {
            let owner = ground.gate_owner(gate);
            if self.gate_owner.insert(gate, owner) != Some(owner) {
                self.closures.clear();
            }
        }
    }

    fn labelled(&mut self, ground: &dyn RegionGround, input: label::Input) -> ChunkRegions {
        let done = label::label(&input, self.bucket);
        self.refresh_owners(ground, &input.classes);
        ChunkRegions {
            signature: input.signature,
            labels: done.labels,
            steps: input.steps,
            comp: vec![0; done.gates.len()],
            gates: done.gates,
            local: done.local,
            links: Vec::new(),
        }
    }

    fn build(&mut self, ground: &dyn RegionGround) {
        self.stats.builds += 1;
        self.dirty.clear();
        let count = (self.chunks_x * self.chunks_y) as u32;
        let mut chunks = Vec::with_capacity(count as usize);
        for chunk in 0..count {
            let key = self.key(chunk);
            let input = label::input(ground, key, self.size);
            chunks.push(self.labelled(ground, input));
        }
        self.chunks = chunks;
        for chunk in 0..count {
            self.link(chunk);
        }
        let supers_x = (self.chunks_x + SUPER - 1) / SUPER;
        let supers_y = (self.chunks_y + SUPER - 1) / SUPER;
        self.supers = (0..supers_x * supers_y)
            .map(|_| SuperRegions::default())
            .collect();
        for at in 0..self.supers.len() as u32 {
            self.rebuild_super(at);
        }
        self.rebuild_global();
    }

    /// Recompute outgoing border links. Reversible links carry a flag so
    /// union-find can join them; directed drops remain graph edges.
    fn link(&mut self, chunk: u32) {
        let at = label::origin(self.key(chunk));
        let mut links = Vec::new();
        let border = (0..CHUNK)
            .map(|x| TileCoord::new(at.x + x, at.y + CHUNK - 1))
            .chain((0..CHUNK - 1).map(|y| TileCoord::new(at.x + CHUNK - 1, at.y + y)))
            .chain((0..CHUNK - 1).map(|y| TileCoord::new(at.x, at.y + y)))
            .chain((1..CHUNK - 1).map(|x| TileCoord::new(at.x + x, at.y)));
        for tile in border {
            let Some((_, own)) = self.region_at(tile) else {
                continue;
            };
            for (direction, (dx, dy)) in label::NEIGHBOURS.into_iter().enumerate() {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                let Some((other, label)) = self.region_at(next) else {
                    continue;
                };
                if other != chunk {
                    let forward = self.chunks[chunk as usize]
                        .steps
                        .at(label::index(tile))
                        .allows(direction, self.bucket);
                    let reverse = self.chunks[other as usize]
                        .steps
                        .at(label::index(next))
                        .allows(7 - direction, self.bucket);
                    if forward {
                        links.push((own, other, label, reverse));
                    }
                }
            }
        }
        links.sort_unstable();
        links.dedup();
        self.chunks[chunk as usize].links = links;
    }

    /// The canonical component of a passable tile, `None` when blocked.
    pub fn component(&mut self, ground: &dyn RegionGround, tile: TileCoord) -> Option<u32> {
        self.flush(ground);
        self.node(tile).map(|node| self.global.root(node))
    }

    /// The component `player` sees a passable tile in: its gates joined.
    pub fn player_component(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        tile: TileCoord,
    ) -> Option<u32> {
        self.flush(ground);
        let root = self.global.root(self.node(tile)?);
        Some(self.closure_root(player, root))
    }

    /// True when `player` can walk from `from` to `to` on today's ground.
    pub fn reachable(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
    ) -> bool {
        if self.chunks.is_empty() && self.clear_ray(ground, player, from, to) {
            return true;
        }
        self.flush(ground);
        let Some(goal) = self.node(to) else {
            return false;
        };
        let goal = self.closure_root(player, self.global.root(goal));
        self.reachable_roots(player, from).contains(&goal)
    }
}

#[path = "regions/tests.rs"]
#[cfg(test)]
mod tests;
