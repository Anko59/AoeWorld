#[cfg(test)]
use aoe_protocol::MAX_RESOURCE_CHANGES;
use aoe_protocol::{MAX_RESOURCE_CACHE, ResourceAmount, ResourceState};

#[cfg(test)]
#[path = "resource_state/tests/paging.rs"]
mod paging_tests;
#[cfg(test)]
#[path = "resource_state/tests/tree_stages.rs"]
mod tree_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeStage {
    Full,
    Chopped,
    Stump,
    Hidden,
}

/// Independent of immutable chunk residency, so evicting terrain never
/// resurrects a depleted resource. Unknown state is hidden until synchronized.
#[derive(Default)]
pub struct ResourceStateCache {
    revision: Option<u64>,
    pending: Option<(u64, u64, usize, Vec<ResourceAmount>)>,
    amounts: Vec<ResourceAmount>,
    exhausted_at: std::collections::BTreeMap<u64, f64>,
}

impl ResourceStateCache {
    #[cfg(target_arch = "wasm32")]
    pub fn starting_nodes(&self) -> impl Iterator<Item = aoe_map::ResourceNode> + '_ {
        self.amounts
            .iter()
            .filter(|amount| amount.remaining > 0)
            .filter_map(|amount| aoe_map::ResourceNode::starting_from_id(amount.id))
    }
    pub fn clear(&mut self) {
        self.revision = None;
        self.pending = None;
        self.amounts.clear();
        self.exhausted_at.clear();
    }
    #[inline(never)]
    pub fn remaining(&self, id: u64, initial: u16) -> u16 {
        self.amounts
            .binary_search_by_key(&id, |value| value.id)
            .map_or(initial, |index| self.amounts[index].remaining)
    }
    pub fn visible(&self, id: u64) -> bool {
        self.revision.is_some() && self.remaining(id, 1) > 0
    }
    pub fn apply(&mut self, state: ResourceState) -> bool {
        self.apply_at(state, 0.0)
    }
    /// Local presentation time only; snapshots never replay historical effects.
    pub fn apply_at(&mut self, state: ResourceState, now_ms: f64) -> bool {
        if state.snapshot_page.is_some() {
            return self.apply_page(state);
        }
        self.pending = None;
        let snapshot = state.from_revision.is_none();
        let exhausted = state
            .changes
            .iter()
            .filter(|change| !snapshot && change.remaining == 0 && self.remaining(change.id, 1) > 0)
            .map(|change| change.id)
            .collect::<Vec<_>>();
        if !self.apply_amounts(state) {
            return false;
        }
        if snapshot {
            self.exhausted_at.clear();
        }
        for id in exhausted {
            self.exhausted_at.insert(id, now_ms);
        }
        true
    }
    pub fn tree_stage(&self, id: u64, initial: u16, now_ms: f64) -> TreeStage {
        if self.revision.is_none() {
            return TreeStage::Hidden;
        }
        match self.remaining(id, initial) {
            0 if self
                .exhausted_at
                .get(&id)
                .is_some_and(|at| now_ms - at < 3000.0) =>
            {
                TreeStage::Stump
            }
            0 => TreeStage::Hidden,
            remaining if remaining < initial => TreeStage::Chopped,
            _ => TreeStage::Full,
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn rendered_starting_nodes(
        &self,
        now_ms: f64,
    ) -> impl Iterator<Item = aoe_map::ResourceNode> + '_ {
        self.amounts
            .iter()
            .filter_map(|amount| aoe_map::ResourceNode::starting_from_id(amount.id))
            .filter(move |node| {
                self.visible(node.id)
                    || (node.kind == aoe_map::ResourceKind::Wood
                        && self.tree_stage(node.id, node.initial_amount, now_ms)
                            == TreeStage::Stump)
            })
    }
    fn apply_page(&mut self, mut state: ResourceState) -> bool {
        if !state.valid() || self.revision.is_some_and(|old| state.revision < old) {
            return false;
        }
        let Some((offset, total)) = state.snapshot_page else {
            return false;
        };
        let mut pending = if offset == 0 {
            Vec::with_capacity(total as usize)
        } else {
            let Some((subscription, revision, count, amounts)) = self.pending.take() else {
                return false;
            };
            if subscription != state.subscription_revision
                || revision != state.revision
                || count != total as usize
                || amounts.len() != offset as usize
                || amounts
                    .last()
                    .zip(state.changes.first())
                    .is_some_and(|(a, b)| a.id >= b.id)
            {
                return false;
            }
            amounts
        };
        pending.append(&mut state.changes);
        if pending.len() < total as usize {
            self.pending = Some((
                state.subscription_revision,
                state.revision,
                total as usize,
                pending,
            ));
            return true;
        }
        state.changes = pending;
        state.snapshot_page = None;
        // Each page already passed the per-message bound; the assembled state
        // is retained data, validated here without pretending it is one frame.
        if state
            .changes
            .iter()
            .filter(|c| c.id & (1 << 37) == 0)
            .count() as u64
            > state.revision
        {
            return false;
        }
        self.amounts = state.changes;
        self.revision = Some(state.revision);
        self.exhausted_at.clear();
        true
    }
    fn apply_amounts(&mut self, state: ResourceState) -> bool {
        if !state.valid()
            || self
                .revision
                .is_some_and(|revision| state.revision < revision)
            || state
                .from_revision
                .is_some_and(|from| self.revision != Some(from))
        {
            return false;
        }
        if state.from_revision.is_none() {
            self.amounts = state.changes;
        } else {
            if state.changes.iter().any(|change| {
                self.amounts
                    .binary_search_by_key(&change.id, |value| value.id)
                    .is_ok_and(|index| change.remaining > self.amounts[index].remaining)
            }) {
                return false;
            }
            let added = state
                .changes
                .iter()
                .filter(|change| {
                    self.amounts
                        .binary_search_by_key(&change.id, |value| value.id)
                        .is_err()
                })
                .count();
            if self.amounts.len() + added > MAX_RESOURCE_CACHE {
                return false;
            }
            if added == 0 {
                let mut next = 0;
                for current in &mut self.amounts {
                    if next < state.changes.len() && current.id == state.changes[next].id {
                        *current = state.changes[next];
                        next += 1;
                    }
                }
            } else {
                let mut merged = Vec::with_capacity(self.amounts.len() + added);
                let mut old = 0;
                for change in state.changes {
                    while old < self.amounts.len() && self.amounts[old].id < change.id {
                        merged.push(self.amounts[old]);
                        old += 1;
                    }
                    if old < self.amounts.len() && self.amounts[old].id == change.id {
                        old += 1;
                    }
                    merged.push(change);
                }
                merged.extend_from_slice(&self.amounts[old..]);
                self.amounts = merged;
            }
        }
        self.revision = Some(state.revision);
        true
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(target_arch = "wasm32")]
#[path = "resource_state/tests/starting_nodes.rs"]
mod starting_tests;
