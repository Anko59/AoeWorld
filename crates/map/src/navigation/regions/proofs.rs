//! Exact directed answers shared by walkers and contacts on the same regions.
use super::{Regions, label};
use aoe_core::TileCoord;
use std::hash::{DefaultHasher, Hash, Hasher};

pub(super) type Node = (u32, u16);
const SLOTS: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Query(pub(super) u32, pub(super) Node, pub(super) Node);

impl Query {
    fn slot(self) -> usize {
        let mut hash = DefaultHasher::new();
        self.hash(&mut hash);
        hash.finish() as usize % SLOTS
    }
}

#[derive(Clone, Copy, Debug)]
struct Proof {
    query: Query,
    generation: u64,
    reachable: bool,
}

#[derive(Debug, Default)]
pub(super) struct Proofs {
    generation: u64,
    slots: Box<[Option<Proof>]>,
}

impl Proofs {
    pub(super) fn invalidate(&mut self) {
        // An edit anywhere on the connecting path (or between failed
        // frontiers) can change its answer. Leave old slots to be replaced.
        self.generation += 1;
    }

    pub(super) fn get(&self, query: Query) -> Option<bool> {
        let proof = self.slots.get(query.slot())?.as_ref()?;
        (proof.generation == self.generation && proof.query == query).then_some(proof.reachable)
    }

    pub(super) fn insert(&mut self, query: Query, reachable: bool) {
        if self.slots.is_empty() {
            self.slots = vec![None; SLOTS].into_boxed_slice();
        }
        self.slots[query.slot()] = Some(Proof {
            query,
            generation: self.generation,
            reachable,
        });
    }
}

impl Regions {
    fn current_lazy_node(&self, tile: TileCoord) -> Option<Node> {
        let chunk = self.chunk_of(tile)?;
        if self.pending_dirty.contains(&chunk) {
            return None;
        }
        let label = self.pending.get(&chunk)?.labels.at(label::index(tile));
        (label != 0).then_some((chunk, label))
    }

    pub(super) fn current_query(
        &self,
        player: u32,
        from: TileCoord,
        to: TileCoord,
    ) -> Option<Query> {
        Some(Query(
            player,
            self.current_lazy_node(from)?,
            self.current_lazy_node(to)?,
        ))
    }

    pub(super) fn known_reach(&self, query: Query) -> Option<bool> {
        let Query(_, from, to) = query;
        if from == to {
            let gate = self.pending[&from.0].gates[usize::from(from.1) - 1];
            return Some(!self.locked(gate));
        }
        self.proofs.get(query)
    }
}
