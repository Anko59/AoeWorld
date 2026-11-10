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
//! the ground beside them only for the players the gate admits. Ownership
//! and open state affect only those joins, so toggling a gate changes no
//! label. Changed chunks are batched and relabelled
//! on the next query; a chunk whose tile classes did not change keeps
//! everything.
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

mod bucket;
mod frontier;
use bucket::BucketEdges;
pub use bucket::LandBucket;
mod label;
mod lazy;
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
    /// False after a provider failure; callers discard all derived labels.
    fn available(&self) -> bool {
        true
    }
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
    /// The owner may pass even while the gate is closed.
    fn gate_owner(&self, gate: u32) -> u32;
    /// An open gate admits every player. Invalidate its tiles when this changes.
    fn gate_open(&self, _gate: u32) -> bool {
        false
    }
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

#[derive(Clone, Debug)]
enum Cells<T> {
    Uniform(T),
    Tiles(Box<[T]>),
}
type Labels = Cells<u16>;
type Steps = Cells<BucketEdges>;

/// A straight witness supplies local routing; a graph witness certifies only
/// connectivity, so a detour can still need corridor guidance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionReachability {
    Unknown,
    Unreachable,
    Connected,
    Straight,
}

impl RegionReachability {
    pub fn is_reachable(self) -> bool {
        matches!(self, Self::Connected | Self::Straight)
    }
}
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

#[derive(Clone, Debug)]
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
    /// Demand-built labels, moved into the dense census by an explicit flush.
    pending: BTreeMap<u32, ChunkRegions>,
    /// Lazy lookups and the dense census each retain their invalidations.
    pending_dirty: BTreeSet<u32>,
    dirty: BTreeSet<u32>,
    supers: Vec<SuperRegions>,
    global: Global,
    gate_access: BTreeMap<u32, (u32, bool)>,
    /// Per player: gated edges admitted, folded into a small union-find.
    closures: BTreeMap<u32, BTreeMap<u32, u32>>,
    stats: RegionStats,
}

impl Regions {
    /// An empty layer; endpoint queries label only chunks they visit.
    pub fn new(width: i32, height: i32, bucket: LandBucket) -> Self {
        let chunks_x = (width.max(1) + CHUNK - 1) / CHUNK;
        let chunks_y = (height.max(1) + CHUNK - 1) / CHUNK;
        Self {
            bucket,
            size: (width.max(1), height.max(1)),
            chunks_x,
            chunks_y,
            chunks: Vec::new(),
            pending: BTreeMap::new(),
            pending_dirty: BTreeSet::new(),
            dirty: BTreeSet::new(),
            supers: Vec::new(),
            global: Global::default(),
            gate_access: BTreeMap::new(),
            closures: BTreeMap::new(),
            stats: RegionStats::default(),
        }
    }

    pub fn stats(&self) -> RegionStats {
        self.stats
    }

    /// Ground in `area` may have changed (tile-exclusive maximum).
    pub fn invalidate(&mut self, min: TileCoord, max: TileCoord) {
        let x0 = (min.x - 1).max(0).div_euclid(CHUNK);
        let y0 = (min.y - 1).max(0).div_euclid(CHUNK);
        let x1 = (max.x).min(self.size.0 - 1).div_euclid(CHUNK);
        let y1 = (max.y).min(self.size.1 - 1).div_euclid(CHUNK);
        for cy in y0..=y1.min(self.chunks_y - 1) {
            for cx in x0..=x1.min(self.chunks_x - 1) {
                let chunk = (cy * self.chunks_x + cx) as u32;
                if self.pending.contains_key(&chunk) || !self.chunks.is_empty() {
                    self.dirty.insert(chunk);
                }
                if self.pending.contains_key(&chunk) {
                    self.pending_dirty.insert(chunk);
                }
            }
        }
    }

    /// Everything may have changed.
    pub fn invalidate_all(&mut self) {
        self.pending_dirty.extend(self.pending.keys().copied());
        self.dirty.extend(self.pending.keys().copied());
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
            self.refresh_gate_access(ground, &input.classes);
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

    fn refresh_gate_access(&mut self, ground: &dyn RegionGround, classes: &[RegionTile]) {
        let gates: BTreeSet<_> = classes
            .iter()
            .filter_map(|class| match class {
                RegionTile::Gate(gate) => Some(*gate),
                _ => None,
            })
            .collect();
        for gate in gates {
            let access = (ground.gate_owner(gate), ground.gate_open(gate));
            if self.gate_access.insert(gate, access) != Some(access) {
                self.closures.clear();
            }
        }
    }

    fn labelled(&mut self, ground: &dyn RegionGround, input: label::Input) -> ChunkRegions {
        let done = label::label(&input, self.bucket);
        self.refresh_gate_access(ground, &input.classes);
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
        self.stats.builds = self.stats.builds.max(1);
        let pending: Vec<_> = self.pending.keys().copied().collect();
        for chunk in pending {
            self.load(ground, chunk);
        }
        self.dirty.clear();
        let count = (self.chunks_x * self.chunks_y) as u32;
        let mut chunks = Vec::with_capacity(count as usize);
        for chunk in 0..count {
            let done = if let Some(done) = self.pending.remove(&chunk) {
                done
            } else {
                let input = label::input(ground, self.key(chunk), self.size);
                self.labelled(ground, input)
            };
            chunks.push(done);
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

    /// The component `player` sees: owned gates and all open gates joined.
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

    /// A bounded optional certificate. Exhaustion is unknown, never a proof
    /// of disconnection. The node allowance counts cached labels too, so
    /// warming changes cost but cannot change the returned verdict.
    pub fn reachable_bounded(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
        limit: u32,
    ) -> RegionReachability {
        if limit == 0 {
            return RegionReachability::Unknown;
        }
        let distance = (i64::from(from.x) - i64::from(to.x))
            .abs()
            .max((i64::from(from.y) - i64::from(to.y)).abs());
        if distance <= i64::from(limit) * i64::from(CHUNK)
            && self.clear_ray(ground, player, from, to)
        {
            return RegionReachability::Straight;
        }
        match self.lazy_reachable_bounded(ground, player, from, to, limit) {
            None => RegionReachability::Unknown,
            Some(false) => RegionReachability::Unreachable,
            Some(true) => RegionReachability::Connected,
        }
    }

    /// True when `player` can walk from `from` to `to` on today's ground.
    pub fn reachable(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
    ) -> bool {
        if self.chunks.is_empty() {
            return self.clear_ray(ground, player, from, to)
                || self.lazy_reachable(ground, player, from, to);
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
