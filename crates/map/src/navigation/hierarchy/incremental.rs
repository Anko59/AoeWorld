//! Resumable portal search: two charged local endpoint attachments, then
//! cheap graph rows. A prepared graph is pinned across polls so depletion
//! cannot change a retained portal's identity. Closures touching its frontier
//! cancel the planner; later local refinement always uses current collision.
use super::*;
use std::collections::BTreeSet;

const ENDPOINT_WORK: u32 = (CHUNK_TILES * CHUNK_TILES * 9 + CHUNK_TILES * 8) as u32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Search {
    origin: TileCoord,
    destination: TileCoord,
    phase: u8,
    credit: u32,
    stage_work: u32,
    expansions: u32,
    max_expansions: u32,
    peak_entries: usize,
    atlas: Option<Arc<prepared::Atlas>>,
    atlas_checked: bool,
    chunks: BTreeSet<Key>,
    preparing: Option<Key>,
    start_costs: BTreeMap<TileCoord, u32>,
    goal_costs: BTreeMap<TileCoord, u32>,
    best: BTreeMap<Node, u64>,
    parent: BTreeMap<Node, Node>,
    heap: BTreeSet<(u64, u64, Node)>,
}

impl Search {
    pub(crate) fn with_preparation(
        origin: TileCoord,
        destination: TileCoord,
        maximum: u32,
        atlas: Arc<prepared::Atlas>,
        chunks: BTreeSet<Key>,
    ) -> Self {
        let mut search = Self::new(origin, destination, maximum);
        search.atlas = Some(atlas);
        search.atlas_checked = true;
        search.chunks = chunks;
        search
    }

    pub(crate) fn into_resolution(
        mut self,
        destination: TileCoord,
    ) -> Option<prepared::Resolution> {
        let mut resolution =
            prepared::Resolution::new(self.atlas.take()?, self.origin, destination);
        resolution.inherit_chunks(self.chunks);
        Some(resolution)
    }

    pub(crate) fn new(origin: TileCoord, destination: TileCoord, max_expansions: u32) -> Self {
        Self {
            origin,
            destination,
            phase: 0,
            credit: 0,
            stage_work: 0,
            expansions: 0,
            max_expansions,
            peak_entries: 2,
            atlas: None,
            atlas_checked: false,
            chunks: BTreeSet::new(),
            preparing: None,
            start_costs: BTreeMap::new(),
            goal_costs: BTreeMap::new(),
            best: BTreeMap::from([(Node::Start, 0)]),
            parent: BTreeMap::new(),
            heap: BTreeSet::from([(
                HEURISTIC_WEIGHT * heuristic(origin, destination),
                u64::MAX,
                Node::Start,
            )]),
        }
    }

    pub(crate) fn touches(&self, area: aoe_core::TileRect) -> bool {
        let overlaps = |tile: TileCoord| {
            let key = chunk_of(tile);
            let (x, y) = (key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
            x < area.max.x
                && y < area.max.y
                && x + CHUNK_TILES > area.min.x
                && y + CHUNK_TILES > area.min.y
        };
        overlaps(self.origin)
            || overlaps(self.destination)
            || self.chunks.iter().any(|key| overlaps(grid::tile(*key, 0)))
            || self
                .preparing
                .is_some_and(|key| overlaps(grid::tile(key, 0)))
            || self
                .best
                .keys()
                .any(|node| matches!(node,Node::Portal(tile) if overlaps(*tile)))
    }

    pub(crate) fn retained_entries(&self) -> usize {
        (self.chunks.len()
            + usize::from(self.preparing.is_some())
            + self.start_costs.len()
            + self.goal_costs.len()
            + self.best.len()
            + self.parent.len()
            + self.heap.len())
        .max(self.peak_entries)
    }

    pub(crate) fn poll(
        &mut self,
        generator: &MapChunkGenerator,
        remaining: &mut u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<HierarchicalOutcome>> {
        self.poll_overlay(generator, &ResourceOverlay::default(), remaining, cancelled)
    }

    pub(crate) fn poll_overlay(
        &mut self,
        generator: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        remaining: &mut u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<HierarchicalOutcome>> {
        let _span = generator.measure_navigation(crate::NavigationPhase::Hierarchy);
        if !self.atlas_checked {
            self.atlas = generator.prepared_navigation(overlay);
            self.atlas_checked = true;
        }
        let mut ground = Ground::new(generator, overlay, cancelled);
        ground.atlas = self.atlas.clone();
        loop {
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            if *remaining == 0 {
                return Ok(None);
            }
            if self.credit == 0 {
                let key = match self.heap.first().map(|entry| entry.2) {
                    Some(Node::Start) => Some(chunk_of(self.origin)),
                    Some(Node::Portal(tile)) => Some(chunk_of(tile)),
                    _ => None,
                };
                self.preparing = key.filter(|key| self.phase >= 2 && !self.chunks.contains(key));
                self.stage_work = if self.preparing.is_some() {
                    prepared::CHUNK_WORK
                } else if self.phase < 2 {
                    ENDPOINT_WORK
                } else {
                    match self.heap.first().map(|entry| entry.2) {
                        Some(Node::Portal(tile)) => {
                            ground.portals(chunk_of(tile))?.portals.len() as u32 + 2
                        }
                        Some(Node::Start) => {
                            ground.portals(chunk_of(self.origin))?.portals.len() as u32 + 2
                        }
                        _ => 1,
                    }
                };
            }
            let take = (*remaining).min(self.stage_work - self.credit);
            self.credit += take;
            *remaining -= take;
            if self.credit < self.stage_work {
                return Ok(None);
            }
            self.credit = 0;
            if let Some(key) = self.preparing.take() {
                ground.portals(key)?;
                self.chunks.insert(key);
                self.peak_entries = self.retained_entries();
                continue;
            }
            let outcome = self.step(&ground)?;
            self.peak_entries = self.retained_entries();
            if outcome.is_some() {
                return Ok(outcome);
            }
        }
    }

    fn step(&mut self, ground: &Ground<'_>) -> Result<Option<HierarchicalOutcome>> {
        let (start_key, goal_key) = (chunk_of(self.origin), chunk_of(self.destination));
        if self.phase == 0 {
            if !ground.is_open(self.origin)? || !ground.is_open(self.destination)? {
                return Ok(Some(HierarchicalOutcome::Invalid));
            }
            self.start_costs = ground.local_costs(start_key, self.origin)?;
            self.phase = 1;
            return Ok(None);
        }
        if self.phase == 1 {
            if start_key == goal_key
                && let Some(cost) = self.start_costs.get(&self.destination)
            {
                return Ok(Some(HierarchicalOutcome::Path {
                    waypoints: Vec::new(),
                    cost: u64::from(*cost),
                }));
            }
            self.goal_costs = ground.goal_costs(goal_key, self.destination)?;
            self.phase = 2;
            return Ok(None);
        }
        let Some((_, inverse_cost, node)) = self.heap.pop_first() else {
            return Ok(Some(HierarchicalOutcome::Unreachable));
        };
        let cost = u64::MAX - inverse_cost;
        if self.best.get(&node) != Some(&cost) {
            return Ok(None);
        }
        if node == Node::Goal {
            let mut waypoints = Vec::new();
            let mut at = node;
            while let Some(&before) = self.parent.get(&at) {
                if let Node::Portal(tile) = at {
                    waypoints.push(tile);
                }
                at = before;
            }
            waypoints.reverse();
            return Ok(Some(HierarchicalOutcome::Path { waypoints, cost }));
        }
        self.expansions += 1;
        if self.expansions > self.max_expansions || self.retained_entries() > 65_536 {
            return Ok(Some(HierarchicalOutcome::Bounded));
        }
        let mut edges = Vec::new();
        match node {
            Node::Start => {
                for portal in &ground.portals(start_key)?.portals {
                    if let Some(cost) = self.start_costs.get(&portal.tile) {
                        edges.push((Node::Portal(portal.tile), u64::from(*cost)));
                    }
                }
            }
            Node::Goal => {}
            Node::Portal(tile) => {
                let key = chunk_of(tile);
                let chunk = ground.portals(key)?;
                let costs = ground.row(key, &chunk, tile)?;
                for (to, other) in chunk.portals.iter().enumerate() {
                    if other.tile != tile
                        && let Some(cost) = costs.get(to).copied().flatten()
                    {
                        edges.push((Node::Portal(other.tile), u64::from(cost)));
                    }
                    if other.tile == tile && other.crossing_cost > 0 {
                        edges.push((Node::Portal(other.cross), u64::from(other.crossing_cost)));
                    }
                }
                if key == goal_key
                    && let Some(cost) = self.goal_costs.get(&tile)
                {
                    edges.push((Node::Goal, u64::from(*cost)));
                }
            }
        }
        for (next, step) in edges {
            let total = cost + step;
            if self.best.get(&next).is_none_or(|known| *known > total) {
                self.best.insert(next, total);
                self.parent.insert(next, node);
                let position = match next {
                    Node::Start => self.origin,
                    Node::Goal => self.destination,
                    Node::Portal(tile) => tile,
                };
                self.heap.insert((
                    total + HEURISTIC_WEIGHT * heuristic(position, self.destination),
                    u64::MAX - total,
                    next,
                ));
            }
        }
        Ok(None)
    }

    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        // Search state is derived deterministically from the endpoints and the
        // number of charged stages, independent of cache state or poll slices.
        for tile in [self.origin, self.destination] {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
        }
        hash.update(&[u8::from(self.atlas_checked)]);
        if let Some(atlas) = &self.atlas {
            hash.update(b"prepared-graph");
            hash.update(&atlas.stamp());
        }
        hash.update(&[self.phase]);
        hash.update(&self.credit.to_le_bytes());
        hash.update(&self.stage_work.to_le_bytes());
        hash.update(&self.expansions.to_le_bytes());
        hash.update(&self.max_expansions.to_le_bytes());
        hash.update(&(self.chunks.len() as u64).to_le_bytes());
        for key in &self.chunks {
            hash.update(&key.0.to_le_bytes());
            hash.update(&key.1.to_le_bytes());
        }
        hash.update(&[u8::from(self.preparing.is_some())]);
        if let Some(key) = self.preparing {
            hash.update(&key.0.to_le_bytes());
            hash.update(&key.1.to_le_bytes());
        }
        hash.update(&(self.peak_entries as u64).to_le_bytes());
        hash.update(&(self.start_costs.len() as u64).to_le_bytes());
        for (tile, cost) in &self.start_costs {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
            hash.update(&cost.to_le_bytes());
        }
        hash.update(&(self.goal_costs.len() as u64).to_le_bytes());
        for (tile, cost) in &self.goal_costs {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
            hash.update(&cost.to_le_bytes());
        }
        hash.update(&(self.best.len() as u64).to_le_bytes());
        for (node, cost) in &self.best {
            hash_node(hash, *node);
            hash.update(&cost.to_le_bytes());
        }
        hash.update(&(self.parent.len() as u64).to_le_bytes());
        for (node, parent) in &self.parent {
            hash_node(hash, *node);
            hash_node(hash, *parent);
        }
        hash.update(&(self.heap.len() as u64).to_le_bytes());
        for (estimate, inverse, node) in &self.heap {
            hash.update(&estimate.to_le_bytes());
            hash.update(&inverse.to_le_bytes());
            hash_node(hash, *node);
        }
    }
}

fn hash_node(hash: &mut blake3::Hasher, node: Node) {
    match node {
        Node::Start => {
            hash.update(&[0]);
        }
        Node::Goal => {
            hash.update(&[1]);
        }
        Node::Portal(tile) => {
            hash.update(&[2]);
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
        }
    }
}
