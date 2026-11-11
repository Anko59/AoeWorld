//! Demand-built chunk labels. Connectivity walks the shared region graph from
//! both ends: a small sealed goal is rejected without labelling its continent.
use super::{CHUNK, RegionGround, Regions, label};
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

type Node = (u32, u16);

fn squared(a: TileCoord, b: TileCoord) -> i64 {
    let (dx, dy) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    dx * dx + dy * dy
}

/// Border cells of one chunk in ascending cell order.
pub(super) fn border_cells() -> impl Iterator<Item = usize> {
    let side = CHUNK as usize;
    (0..side)
        .chain((1..side - 1).flat_map(move |y| [y * side, y * side + side - 1]))
        .chain((side - 1) * side..side * side)
}

impl Regions {
    pub(super) fn load(&mut self, ground: &dyn RegionGround, chunk: u32) {
        let invalidated = self.pending_dirty.remove(&chunk);
        if self.pending.contains_key(&chunk) && !invalidated {
            return;
        }
        if !self.chunks.is_empty() && !self.dirty.contains(&chunk) {
            self.pending
                .insert(chunk, self.chunks[chunk as usize].clone());
            return;
        }
        self.stats.builds = self.stats.builds.max(1);
        let input = label::input(ground, self.key(chunk), self.size);
        self.refresh_gate_access(ground, &input.classes);
        if let Some(previous) = self.pending.get(&chunk) {
            if previous.signature == input.signature {
                self.stats.unchanged += 1;
                return;
            }
            self.stats.relabels += 1;
        }
        let done = self.labelled(ground, input);
        self.pending.insert(chunk, done);
    }

    fn lazy_node(&mut self, ground: &dyn RegionGround, tile: TileCoord) -> Option<Node> {
        let chunk = self.chunk_of(tile)?;
        self.load(ground, chunk);
        let label = self.pending[&chunk].labels.at(label::index(tile));
        (label != 0).then_some((chunk, label))
    }

    fn allowed_node(&self, player: u32, node: Node, start: Node) -> bool {
        let gate = self.pending[&node.0].gates[usize::from(node.1) - 1];
        let initial = self.pending[&start.0].gates[usize::from(start.1) - 1];
        node == start || (gate.is_some() && gate == initial) || self.admits(player, gate)
    }

    fn adjacent(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        node: Node,
        start: Node,
        reverse: bool,
    ) -> BTreeSet<Node> {
        let (chunk, own) = node;
        let mut out = BTreeSet::new();
        for &(a, b, both) in &self.pending[&chunk].local {
            if (a == own && !reverse) || (b == own && reverse) {
                out.insert((chunk, if reverse { a } else { b }));
            }
            if both && ((b == own && !reverse) || (a == own && reverse)) {
                out.insert((chunk, if reverse { b } else { a }));
            }
        }
        let at = label::origin(self.key(chunk));
        // Only border cells can step into another chunk. Visit them in
        // ascending cell order, as a full scan would, and collect crossings
        // before loading so each neighbouring chunk is loaded once, in order
        // of first use. Loading never changes this chunk's labels or steps.
        let mut crossings = Vec::new();
        let mut others: Vec<u32> = Vec::with_capacity(8);
        let current = &self.pending[&chunk];
        for cell in border_cells() {
            if current.labels.at(cell) != own {
                continue;
            }
            let tile = TileCoord::new(at.x + cell as i32 % CHUNK, at.y + cell as i32 / CHUNK);
            for (direction, (dx, dy)) in label::NEIGHBOURS.into_iter().enumerate() {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                let Some(other) = self.chunk_of(next).filter(|&other| other != chunk) else {
                    continue;
                };
                // A forward edge is already captured in this chunk's halo.
                // Reverse edges must read the neighbouring chunk's step mask.
                if !reverse && !current.steps.at(cell).allows(direction, self.bucket) {
                    continue;
                }
                if !others.contains(&other) {
                    others.push(other);
                }
                crossings.push((direction, next, other));
            }
        }
        for &other in &others {
            self.load(ground, other);
        }
        for (direction, next, other) in crossings {
            let neighbour = &self.pending[&other];
            if reverse
                && !neighbour
                    .steps
                    .at(label::index(next))
                    .allows(7 - direction, self.bucket)
            {
                continue;
            }
            let label = neighbour.labels.at(label::index(next));
            if label != 0 {
                out.insert((other, label));
            }
        }
        out.retain(|&next| self.allowed_node(player, next, start));
        out
    }

    fn connected(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        start: Node,
        goal: Node,
    ) -> bool {
        self.connected_bounded(ground, player, start, goal, None)
            .unwrap_or(false)
    }

    fn connected_bounded(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        start: Node,
        goal: Node,
        mut remaining: Option<u32>,
    ) -> Option<bool> {
        if start == goal {
            return Some(true);
        }
        if !self.allowed_node(player, goal, start) {
            return Some(false);
        }
        let mut seen = [BTreeSet::from([start]), BTreeSet::from([goal])];
        let mut queues = [BTreeSet::from([(0, start)]), BTreeSet::from([(0, goal)])];
        let ends = [
            label::origin(self.key(goal.0)),
            label::origin(self.key(start.0)),
        ];
        let mut turn = 0;
        while ground.available() && !queues[0].is_empty() && !queues[1].is_empty() {
            if let Some(left) = &mut remaining {
                if *left == 0 {
                    return None;
                }
                *left -= 1;
            }
            let side = turn % 2;
            turn += 1;
            let Some((_, node)) = queues[side].pop_first() else {
                break;
            };
            for next in self.adjacent(ground, player, node, start, side == 1) {
                if seen[1 - side].contains(&next) {
                    return Some(true);
                }
                if seen[side].insert(next) {
                    let near = label::origin(self.key(next.0));
                    queues[side].insert((squared(near, ends[side]), next));
                }
            }
        }
        Some(false)
    }

    pub(super) fn lazy_reachable_bounded(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
        limit: u32,
    ) -> Option<bool> {
        if limit == 0 {
            return None;
        }
        let Some(start) = self.lazy_node(ground, from) else {
            return Some(false);
        };
        let Some(goal) = self.lazy_node(ground, to) else {
            return Some(false);
        };
        self.connected_bounded(ground, player, start, goal, Some(limit))
    }

    pub(super) fn lazy_reachable(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
    ) -> bool {
        let Some(start) = self.lazy_node(ground, from) else {
            return false;
        };
        let Some(goal) = self.lazy_node(ground, to) else {
            return false;
        };
        self.connected(ground, player, start, goal)
    }

    pub(super) fn lazy_frontier(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        target: TileCoord,
    ) -> Option<TileCoord> {
        let start = self.lazy_node(ground, from)?;
        if let Some(goal) = self.lazy_node(ground, target)
            && self.connected(ground, player, start, goal)
        {
            return Some(target);
        }
        let clamped = TileCoord::new(
            target.x.clamp(0, self.size.0 - 1),
            target.y.clamp(0, self.size.1 - 1),
        );
        let first = self.chunk_of(clamped)?;
        let mut queue = BTreeSet::from([(self.chunk_distance(first, target), first)]);
        let mut seen = BTreeSet::from([first]);
        let mut known = BTreeMap::new();
        let mut best: Option<(i64, i64, i32, i32)> = None;
        while ground.available() {
            let Some((distance, chunk)) = queue.pop_first() else {
                break;
            };
            if best.is_some_and(|(d, ..)| distance > d) {
                break;
            }
            self.load(ground, chunk);
            let at = label::origin(self.key(chunk));
            let mut candidates = Vec::new();
            for cell in 0..super::CELLS {
                let tile = TileCoord::new(at.x + cell as i32 % CHUNK, at.y + cell as i32 / CHUNK);
                let label = self.pending[&chunk].labels.at(cell);
                if label == 0 || self.pending[&chunk].gates[usize::from(label) - 1].is_some() {
                    continue;
                }
                let key = (squared(tile, target), squared(tile, from), tile.y, tile.x);
                if best.is_none_or(|b| key < b) {
                    candidates.push((key, (chunk, label)));
                }
            }
            candidates.sort_unstable();
            for (key, node) in candidates {
                let reachable = match known.get(&node) {
                    Some(&answer) => answer,
                    None => {
                        let answer = self.connected(ground, player, start, node);
                        known.insert(node, answer);
                        answer
                    }
                };
                if reachable {
                    best = Some(key);
                    break;
                }
            }
            let (cx, cy) = self.key(chunk);
            for (dx, dy) in label::NEIGHBOURS {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.chunks_x || y >= self.chunks_y {
                    continue;
                }
                let next = (y * self.chunks_x + x) as u32;
                if seen.insert(next) {
                    queue.insert((self.chunk_distance(next, target), next));
                }
            }
        }
        best.map(|(_, _, y, x)| TileCoord::new(x, y))
    }
}
