//! Sparse, deterministic work at cut forest edges and depleted fishing sites.
use super::{GameWorld, PreparedResourceDepletion};
use crate::Terrain;
use aoe_core::{TileCoord, TileRect, WorldRect};
use aoe_map::{Depletion, ObjectKind, ResourceNode, ResourceOverlay, ResourceOverlaySnapshot};
use std::collections::{BTreeMap, BTreeSet};

const FOREST_SECONDS: u64 = 120;
const FISH_SECONDS: u64 = 180;
const FISH_RECRUITS: u16 = 10;

#[derive(Clone, Debug, Default)]
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

    pub(super) fn write_snapshot(
        &self,
        snapshot: &mut ResourceOverlaySnapshot,
        tick: u64,
        hz: u32,
    ) {
        if self.due.is_empty() {
            return;
        }
        snapshot.recovery_hz = hz;
        snapshot.recovery = self
            .due
            .iter()
            .map(|&(due, id)| (due.saturating_sub(tick), id))
            .collect();
        snapshot.recovery.sort_unstable();
    }

    pub(super) fn from_snapshot(
        snapshot: &ResourceOverlaySnapshot,
        overlay: &ResourceOverlay,
        tick: u64,
        hz: u32,
    ) -> Option<Self> {
        let records = &snapshot.recovery;
        if snapshot.recovery_hz == 0
            || records.len() > overlay.changed_count()
            || records.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return None;
        }
        let mut timers = Self::default();
        for &(remaining, id) in records {
            if overlay.changed_amount(id).is_none() || timers.scheduled.contains_key(&id) {
                return None;
            }
            // A partial tick at the new rate takes a whole step to elapse.
            let remaining =
                (u128::from(remaining) * u128::from(hz)).div_ceil(u128::from(snapshot.recovery_hz));
            let remaining = u64::try_from(remaining).unwrap_or(u64::MAX);
            timers.schedule(id, tick.saturating_add(remaining));
        }
        Some(timers)
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
    fn recovery_deadline(&self, seconds: u64) -> u64 {
        self.tick
            .0
            .saturating_add(seconds * u64::from(self.config.tick_hz))
    }

    pub(super) fn update_resource_recovery(
        &self,
        timers: &mut ResourceRecovery,
        node: ResourceNode,
        change: Depletion,
    ) {
        match node.object {
            ObjectKind::Tree if change.became_nonblocking => {
                timers.schedule(node.id, self.recovery_deadline(FOREST_SECONDS));
            }
            ObjectKind::Tree if change.added > 0 => {
                timers.cancel(node.id);
                for tile in neighbors(node.tile) {
                    if let Some(cut) = self.original_tree(tile)
                        && self.resource_remaining(cut.id) == Some(0)
                    {
                        // Each newly exposed edge waits a full interval.
                        timers.cancel(cut.id);
                        timers.schedule(cut.id, self.recovery_deadline(FOREST_SECONDS));
                    }
                }
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish if change.became_nonblocking => {
                timers.cancel(node.id);
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish if change.added > 0 => {
                timers.cancel(node.id);
                if change.remaining < node.initial_amount {
                    timers.schedule(node.id, self.recovery_deadline(FISH_SECONDS));
                }
            }
            ObjectKind::ShoreFish | ObjectKind::DeepFish
                if change.removed > 0
                    && change.remaining > 0
                    && change.remaining + change.removed == node.initial_amount =>
            {
                timers.schedule(node.id, self.recovery_deadline(FISH_SECONDS));
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
        // Take the seed decision for the whole batch before adding any new
        // trees. Keep viable work queued so each persisted mutation also
        // includes the other deadlines from this batch.
        let (ready, finished): (Vec<_>, Vec<_>) = self
            .economy
            .resource_recovery
            .due
            .range(..=(self.tick.0, u64::MAX))
            .map(|&(_, id)| {
                let node = overlay
                    .resource_by_id_with_cancel(generator, id, &|| false)
                    .ok()
                    .flatten()
                    .filter(|node| self.can_recover_resource(*node));
                (id, node)
            })
            .partition(|(_, node)| node.is_some());
        for (id, _) in finished {
            self.economy.resource_recovery.cancel(id);
        }
        for (_, node) in ready {
            if let Some(node) = node {
                self.recover_resource(node, sink);
            }
        }
    }

    fn can_recover_resource(&self, node: ResourceNode) -> bool {
        match node.object {
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
        }
    }

    fn defer_resource_recovery(&mut self, id: u64, seconds: u64) {
        let due = self.recovery_deadline(seconds);
        let recovery = &mut self.economy.resource_recovery;
        recovery.cancel(id);
        recovery.schedule(id, due);
    }

    fn tree_site_free(&self, tile: TileCoord) -> bool {
        if !self.tile_free(tile)
            || self.tile_has_reservation(tile)
            || self.destination_tile_reserved(tile)
            || self.living_animal_at(tile)
        {
            return false;
        }
        let tile_rect = TileRect::from_xywh(tile.x, tile.y, 1, 1);
        let Ok(cell) = WorldRect::from_tiles(tile_rect) else {
            return false;
        };
        let nearby = TileRect::new(
            TileCoord::new((tile.x - 1).max(0), (tile.y - 1).max(0)),
            TileCoord::new(
                (tile.x + 2).min(self.config.width_tiles),
                (tile.y + 2).min(self.config.height_tiles),
            ),
        );
        !self.query(nearby).0.into_iter().any(|unit| {
            let radius = i64::from(unit.kind.terrain_radius());
            let dx = i64::from(unit.position.x)
                - i64::from(unit.position.x.clamp(cell.min.x, cell.max.x));
            let dy = i64::from(unit.position.y)
                - i64::from(unit.position.y.clamp(cell.min.y, cell.max.y));
            dx * dx + dy * dy < radius * radius
        })
    }

    fn recover_resource(
        &mut self,
        node: ResourceNode,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
    ) {
        let tree = node.object == ObjectKind::Tree;
        let seconds = if tree { FOREST_SECONDS } else { FISH_SECONDS };
        if tree && !self.tree_site_free(node.tile) {
            self.defer_resource_recovery(node.id, seconds);
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
            self.economy.resource_recovery.cancel(node.id);
            return;
        };
        let transaction = PreparedResourceDepletion {
            previous_revision: overlay.revision(),
            world: self,
            depletion,
            node,
        };
        if !sink(&transaction) {
            self.defer_resource_recovery(node.id, seconds);
            return;
        }
        let change = transaction.commit();
        self.economy.depletions.push(change);
    }
}
