//! Compact priority frontier. Only strictly improving tile costs are queued,
//! so every (cost, tile) key is unique without a balanced tree's deduplication.
use super::search::OpenNode;
use std::{cmp::Reverse, collections::BinaryHeap};

#[derive(Clone, Debug, Default)]
pub(super) struct Frontier(BinaryHeap<Reverse<OpenNode>>);

impl Frontier {
    pub(super) fn insert(&mut self, node: OpenNode) {
        self.0.push(Reverse(node));
    }

    pub(super) fn pop_first(&mut self) -> Option<OpenNode> {
        self.0.pop().map(|Reverse(node)| node)
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// Serialization and logical equality read priority order, independently
    /// of insertion history and heap layout. Not used by the search loop.
    pub(super) fn ordered(&self) -> impl Iterator<Item = OpenNode> {
        self.0
            .clone()
            .into_sorted_vec()
            .into_iter()
            .rev()
            .map(|Reverse(node)| node)
    }
}

impl PartialEq for Frontier {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.ordered().eq(other.ordered())
    }
}
impl Eq for Frontier {}
