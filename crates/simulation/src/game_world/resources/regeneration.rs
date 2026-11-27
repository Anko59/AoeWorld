//! Sparse, deterministic work at cut forest edges and depleted fishing sites.
use super::{GameWorld, PreparedResourceDepletion};
use crate::Terrain;
use aoe_core::TileCoord;
use aoe_map::{Depletion, ObjectKind, ResourceNode};
use std::collections::{BTreeMap, BTreeSet};

const FOREST_SECONDS: u64 = 120;
const FISH_SECONDS: u64 = 180;
const FISH_RECRUITS: u16 = 10;

#[derive(Debug, Default)]
pub(crate) struct ResourceRecovery {
    due: BTreeSet<(u64, u64)>,
    scheduled: BTreeMap<u64, u64>,
}

impl ResourceRecovery {
    pub(crate) fn ready(&self, tick: u64) -> bool {
        self.due.first().is_some_and(|&(due, _)| due <= tick)
    }

    fn schedule(&mut self, id: u64, due: u64) {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.scheduled.entry(id) {
            entry.insert(due);
            self.due.insert((due, id));
        }
    }

    pub(crate) fn cancel(&mut self, id: u64) {
        if let Some(due) = self.scheduled.remove(&id) {
            self.due.remove(&(due, id));
        }
    }

    pub(crate) fn records(&self) -> Vec<(u64, u64)> {
        self.due.iter().copied().collect()
    }

    pub(crate) fn hash(&self, hash: &mut blake3::Hasher) {
        if !self.due.is_empty() {
            hash.update(b"resource recovery\0");
            hash.update(&(self.due.len() as u64).to_le_bytes());
            for &(due, id) in &self.due {
                hash.update(&due.to_le_bytes());
                hash.update(&id.to_le_bytes());
            }
        }
    }
}

fn neighbors(tile: TileCoord) -> impl Iterator<Item = TileCoord> {
    [(0, -1), (-1, 0), (1, 0), (0, 1)]
        .into_iter()
        .map(move |(dx, dy)| TileCoord::new(tile.x + dx, tile.y + dy))
}

impl GameWorld {
    pub(crate) fn restore_resource_recovery(&mut self, records: &[(u64, u64)]) -> bool {
        self.economy.resource_recovery = ResourceRecovery::default();
        if records.is_empty() {
            return true;
        }
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return false;
        };
        if records.len() > overlay.changed_count() {
            return false;
        }
        let timers = &mut self.economy.resource_recovery;
        for &(due, id) in records {
            if overlay.changed_amount(id).is_none() || timers.scheduled.contains_key(&id) {
                return false;
            }
            timers.schedule(id, due);
        }
        true
    }

    fn schedule_resource(&mut self, id: u64, seconds: u64) {
        let due = self
            .tick
            .0
            .saturating_add(seconds * u64::from(self.config.tick_hz));
        self.economy.resource_recovery.schedule(id, due);
    }

    pub(super) fn schedule_resource_recovery(&mut self, node: ResourceNode, change: Depletion) {
        match node.object {
            ObjectKind::Tree if change.became_nonblocking => {
                self.schedule_resource(node.id, FOREST_SECONDS);
            }
            ObjectKind::Tree if change.added > 0 => {
                self.economy.resource_recovery.cancel(node.id);
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish if change.became_nonblocking => {
                self.economy.resource_recovery.cancel(node.id);
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish
                if change.removed > 0
                    && change.remaining > 0
                    && change.remaining + change.removed == node.initial_amount =>
            {
                self.schedule_resource(node.id, FISH_SECONDS);
            }
            _ => {}
        }
    }

    /// The original forest footprint consists of cardinally connected tree
    /// tiles, including starting-kit woods. Exhaustion changes the amount,
    /// so the immutable node (or the original starting node) still resolves.
    fn original_tree(&self, tile: TileCoord) -> Option<ResourceNode> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        generator
            .object_at(tile)
            .or_else(|| overlay.starting_nodes_in(tile, tile).next())
            .filter(|node| node.object == ObjectKind::Tree)
    }

    pub(crate) fn advance_resource_recovery(
        &mut self,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
    ) {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return;
        };
        let timers = &mut self.economy.resource_recovery;
        let mut nodes = Vec::new();
        while let Some(&(due, id)) = timers.due.first().filter(|(due, _)| *due <= self.tick.0) {
            timers.due.remove(&(due, id));
            timers.scheduled.remove(&id);
            if let Ok(Some(node)) = overlay.resource_by_id_with_cancel(generator, id, &|| false) {
                nodes.push(node);
            }
        }
        // Take the seed decision for the whole batch before adding any new
        // trees. The next ring gets its own full growth interval.
        nodes.retain(|node| match node.object {
            ObjectKind::Tree => {
                self.resource_remaining(node.id) == Some(0)
                    && neighbors(node.tile).any(|tile| {
                        self.original_tree(tile).is_some_and(|tree| {
                            self.resource_remaining(tree.id)
                                .is_some_and(|amount| amount > 0)
                        })
                    })
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish => self
                .resource_remaining(node.id)
                .is_some_and(|amount| amount > 0 && amount < node.initial_amount),
            _ => false,
        });
        for node in nodes {
            self.recover_resource(node, sink);
        }
    }

    fn recover_resource(
        &mut self,
        node: ResourceNode,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
    ) {
        let tree = node.object == ObjectKind::Tree;
        let seconds = if tree { FOREST_SECONDS } else { FISH_SECONDS };
        if tree
            && (!self.tile_free(node.tile)
                || self.tile_has_reservation(node.tile)
                || self.living_animal_at(node.tile))
        {
            self.schedule_resource(node.id, seconds);
            return;
        }
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return;
        };
        let amount = if tree {
            node.initial_amount
        } else {
            FISH_RECRUITS
        };
        let Ok(Some(depletion)) = overlay.plan_regeneration(node, amount) else {
            return;
        };
        let transaction = PreparedResourceDepletion {
            previous_revision: overlay.revision(),
            world: self,
            depletion,
            node,
        };
        if !sink(&transaction) {
            self.schedule_resource(node.id, seconds);
            return;
        }
        let change = transaction.commit();
        self.economy.depletions.push(change);
        if tree {
            for tile in neighbors(node.tile) {
                if let Some(cut) = self.original_tree(tile)
                    && self.resource_remaining(cut.id) == Some(0)
                {
                    // A newly exposed edge waits a full interval even if an
                    // earlier check for the same vacancy was still queued.
                    self.economy.resource_recovery.cancel(cut.id);
                    self.schedule_resource(cut.id, FOREST_SECONDS);
                }
            }
        } else if change.remaining < node.initial_amount {
            self.schedule_resource(node.id, seconds);
        }
    }
}
