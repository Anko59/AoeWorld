//! Incremental domain discovery. One block costs one ordinary search token.
use super::super::gathering::{BLOCK_TILES, ResourceSearchWork};
use super::{VASSAL_RULES, VassalWork, rules::Budget, snapshot::Snapshot};
use crate::{GameWorld, Terrain, TileVisibility};
use aoe_core::{EntityId, WorldPosition};
use aoe_map::{ObjectKind, ResourceKind, ResourceNode};
use std::collections::BTreeMap;

const CANDIDATES: usize = 8;

#[derive(Clone, Debug, Default)]
pub(super) struct Search {
    after: usize,
    /// Queue order is authoritative: offered nodes rotate to the back, and
    /// new discoveries evict the oldest entry when this small queue is full.
    candidates: [Vec<ResourceNode>; 4],
    dropoff_after: [Option<EntityId>; 4],
    /// At most 100 worker contexts, including prospective idle jobs. A failed
    /// admission never consumes a managed slot; eviction protects managed jobs.
    refused: BTreeMap<EntityId, [Option<u64>; 4]>,
}

impl Search {
    pub(super) fn forget_worker(&mut self, worker: EntityId) {
        self.refused.remove(&worker);
    }

    pub(super) fn clear_workers(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.refused).into_keys().collect()
    }

    fn record_result(&mut self, worker: EntityId, kind: ResourceKind, node: u64, accepted: bool) {
        if accepted {
            if let Some(refused) = self.refused.get_mut(&worker) {
                refused[kind as usize] = None;
                if refused.iter().all(Option::is_none) {
                    self.refused.remove(&worker);
                }
            }
        } else {
            self.refused.entry(worker).or_insert([None; 4])[kind as usize] = Some(node);
        }
        debug_assert!(self.refused.len() <= VASSAL_RULES.max_villagers);
    }

    pub(super) fn update_hash(&self, h: &mut blake3::Hasher) {
        h.update(&(self.after as u64).to_le_bytes());
        h.update(&(self.refused.len() as u64).to_le_bytes());
        for (worker, refused) in &self.refused {
            h.update(&worker.0.to_le_bytes());
            for node in refused {
                h.update(&[u8::from(node.is_some())]);
                if let Some(node) = node {
                    h.update(&node.to_le_bytes());
                }
            }
        }
        for id in self.dropoff_after {
            h.update(&[u8::from(id.is_some())]);
            if let Some(id) = id {
                h.update(&id.0.to_le_bytes());
            }
        }
        for candidates in &self.candidates {
            h.update(&(candidates.len() as u64).to_le_bytes());
            for node in candidates {
                h.update(&node.id.to_le_bytes());
                h.update(&node.tile.x.to_le_bytes());
                h.update(&node.tile.y.to_le_bytes());
                h.update(&[node.kind as u8, node.object as u8, node.visual_variant]);
                h.update(&node.initial_amount.to_le_bytes());
            }
        }
    }
}

impl GameWorld {
    pub(super) fn vassal_resource_result(
        &mut self,
        worker: EntityId,
        kind: ResourceKind,
        node: u64,
        accepted: bool,
        s: &mut Snapshot,
    ) {
        if self
            .economy
            .vassals
            .refusal_owners
            .get(&worker)
            .is_some_and(|owner| *owner != s.vassal)
        {
            self.forget_vassal_refusals(worker);
        }
        if !accepted {
            let home = &self.economy.vassal_homes[&s.vassal];
            if !home.resources.refused.contains_key(&worker)
                && home.resources.refused.len() >= VASSAL_RULES.max_villagers
            {
                // Fixed-size context replacement, never an admission cap. A
                // later productive idle villager may still obtain a real job.
                let evicted = home
                    .resources
                    .refused
                    .keys()
                    .find(|id| !home.workers.contains(*id))
                    .copied();
                let Some(evicted) = evicted else { return };
                self.forget_vassal_refusals(evicted);
                s.home.resources.forget_worker(evicted);
            }
        }
        s.home.resources.record_result(worker, kind, node, accepted);
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.resources.record_result(worker, kind, node, accepted);
        }
        if s.home.resources.refused.contains_key(&worker) {
            self.economy.vassals.refusal_owners.insert(worker, s.vassal);
        } else {
            self.economy.vassals.refusal_owners.remove(&worker);
        }
    }

    /// Reverse lookup keeps release/death/manual cleanup independent of the
    /// number of active domains, including contexts of unclaimed idle workers.
    pub(super) fn forget_vassal_refusals(&mut self, worker: EntityId) {
        if let Some(owner) = self.economy.vassals.refusal_owners.remove(&worker)
            && let Some(home) = self.economy.vassal_homes.get_mut(&owner)
        {
            home.resources.forget_worker(worker);
        }
    }

    pub(super) fn vassal_needs_discovery(&self, s: &Snapshot) -> bool {
        if !matches!(self.terrain, Terrain::Map { .. }) {
            return false;
        }
        let Some(priority) = self.vassal_priority(s.player) else {
            return false;
        };
        let wanted = priority.kinds()[0];
        s.units.iter().any(|u| {
            self.vassal_worker_idle(u.id, s.vassal)
                || (self.vassal_controller(u.id) == Some(s.vassal)
                    && self
                        .gathering_kind(u.id)
                        .is_some_and(|kind| priority.improves(wanted, kind)))
        })
    }

    fn vassal_node_known(&self, n: ResourceNode, s: &Snapshot) -> bool {
        n.object != ObjectKind::DeepFish
            && WorldPosition::from_tile_center(n.tile).is_ok_and(|at| s.home.owns(at))
            && self.visibility(s.player, n.tile) != TileVisibility::Unexplored
            && self
                .resource_remaining(n.id)
                .is_some_and(|amount| amount > 0)
    }

    fn vassal_node_eligible(&self, node: ResourceNode, worker: EntityId, s: &Snapshot) -> bool {
        self.vassal_node_known(node, s)
            && s.avoid.is_none_or(|a| a.avoid_tile != Some(node.tile))
            && self.node_has_work_slot(node.id, worker)
    }

    /// A completed camp is useful existing knowledge when the worker has
    /// moved away from its resource. Rotate stable IDs so a barren first camp
    /// cannot hide another, without resetting the domain block sweep.
    fn vassal_camp_resource(
        &mut self,
        worker: EntityId,
        kind: ResourceKind,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> Option<ResourceNode> {
        let from = self.unit(worker)?.position.tile_floor();
        let anchors: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| b.is_complete() && b.kind.accepts(kind))
            .collect();
        let anchor = if let Some(after) = s.home.resources.dropoff_after[kind as usize] {
            anchors
                .iter()
                .find(|b| b.id > after)
                .or_else(|| anchors.first())
        } else {
            anchors.iter().min_by_key(|b| {
                (
                    super::super::dropoff::ring_distance(b.footprint, from),
                    b.id,
                )
            })
        }?;
        if !budget.search(work) {
            return None;
        }
        let (id, origin) = (anchor.id, anchor.footprint.min);
        s.home.resources.dropoff_after[kind as usize] = Some(id);
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.resources.dropoff_after[kind as usize] = Some(id);
        }
        let mut counted = ResourceSearchWork::default();
        let node = self.nearest_resource_matching_counted(
            origin,
            kind,
            10,
            None,
            |node| self.vassal_node_eligible(*node, worker, s),
            &mut counted,
        );
        work.resource_blocks += counted.blocks;
        work.resource_nodes += counted.nodes;
        node
    }

    /// Stable center-out block identities are independent of worker positions.
    /// Every advance, including an empty or out-of-map block, spends a token.
    pub(super) fn vassal_discover(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return false;
        };
        if !budget.search(work) {
            return false;
        }
        // Keep full caches from permanently hiding later live candidates.
        for kind in super::priority::KINDS {
            let mut candidates = std::mem::take(&mut s.home.resources.candidates[kind as usize]);
            candidates.retain(|node| {
                work.resource_nodes += 1;
                self.vassal_node_known(*node, s)
            });
            s.home.resources.candidates[kind as usize] = candidates;
        }
        let center = s.home.center.tile_floor();
        let r = VASSAL_RULES.radius_tiles;
        let rings = (r as usize).div_ceil(BLOCK_TILES as usize) + 1;
        let count = (2 * rings + 1).pow(2);
        let cursor = s.home.resources.after % count;
        let (dx, dy) = block_offset(cursor);
        let block = (
            center.x.div_euclid(BLOCK_TILES) + dx,
            center.y.div_euclid(BLOCK_TILES) + dy,
        );
        s.home.resources.after = (cursor + 1) % count;
        work.resource_blocks += 1;
        let nodes = self.block_resource_nodes(generator, overlay, block);
        for node in nodes.iter().copied() {
            work.resource_nodes += 1;
            if !self.vassal_node_known(node, s) {
                continue;
            }
            let candidates = &mut s.home.resources.candidates[node.kind as usize];
            if candidates.iter().any(|old| {
                work.resource_nodes += 1;
                old.id == node.id
            }) {
                continue;
            }
            if candidates.len() == CANDIDATES {
                candidates.remove(0);
            }
            candidates.push(node);
        }
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.resources = s.home.resources.clone()
        }
        true
    }

    /// Both the ordinary local query and the small retained cache share a
    /// search admission; every entry inspected contributes to work telemetry.
    pub(super) fn vassal_resource(
        &mut self,
        worker: EntityId,
        kind: ResourceKind,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> Option<ResourceNode> {
        if budget.orders >= VASSAL_RULES.orders_per_domain {
            return None;
        }
        let from = self.unit(worker)?.position.tile_floor();
        let mut searched = false;
        for preference in 0..3 {
            if preference == 1
                && let Some(node) = self.vassal_camp_resource(worker, kind, s, budget, work)
            {
                return self.vassal_offer_resource(worker, kind, Some(node), s, work);
            }
            if !budget.search(work) {
                break;
            }
            searched = true;
            let mut counted = ResourceSearchWork::default();
            let local = self.nearest_resource_matching_counted(
                from,
                kind,
                10,
                None,
                |node| {
                    self.vassal_node_eligible(*node, worker, s)
                        && (preference > 0 || !self.tree_is_crowded(node, None))
                        && (preference > 1 || !self.vassal_crosses_water(from, node.tile))
                },
                &mut counted,
            );
            work.resource_blocks += counted.blocks;
            work.resource_nodes += counted.nodes;
            if local.is_some() {
                return self.vassal_offer_resource(worker, kind, local, s, work);
            }
        }
        if !searched {
            return None;
        }
        self.vassal_offer_resource(worker, kind, None, s, work)
    }

    /// Local and camp probes contribute knowledge to the same rotating queue.
    /// Prefer the probe unless this worker actually had that node refused.
    /// Other workers keep their own useful local/camp preferences. One cache
    /// pass covers duplicates and alternative eligibility, including FIFO repair.
    fn vassal_offer_resource(
        &mut self,
        worker: EntityId,
        kind: ResourceKind,
        incoming: Option<ResourceNode>,
        s: &mut Snapshot,
        work: &mut VassalWork,
    ) -> Option<ResourceNode> {
        let refused = s
            .home
            .resources
            .refused
            .get(&worker)
            .and_then(|nodes| nodes[kind as usize]);
        let prefer_probe = incoming.is_some_and(|node| Some(node.id) != refused);
        let mut candidates = std::mem::take(&mut s.home.resources.candidates[kind as usize]);
        let (mut first, mut second) = (None, None);
        let (mut fallback, mut incoming_index) = (None, None);
        for (index, node) in candidates.iter().copied().enumerate() {
            work.resource_nodes += 1;
            if incoming.is_some_and(|next| next.id == node.id) {
                incoming_index = Some(index);
            }
            if self.vassal_node_eligible(node, worker, s) {
                if Some(node.id) == refused {
                    fallback = Some((index, node));
                } else if first.is_none() {
                    first = Some((index, node));
                } else if second.is_none() {
                    second = Some((index, node));
                }
            }
            if (prefer_probe && incoming_index.is_some())
                || (first.is_some() && (incoming.is_none() || incoming_index.is_some()))
            {
                break;
            }
        }
        if let Some(node) = incoming.filter(|_| incoming_index.is_none()) {
            if candidates.len() == CANDIDATES {
                candidates.remove(0);
                if first.is_some_and(|(index, _)| index == 0) {
                    first = second;
                }
                if let Some((index, _)) = &mut first {
                    *index -= 1;
                }
                fallback = fallback
                    .and_then(|(index, node)| index.checked_sub(1).map(|index| (index, node)));
            }
            let index = candidates.len();
            candidates.push(node);
            incoming_index = Some(index);
        }
        let probe = incoming_index.zip(incoming);
        let chosen = if prefer_probe {
            probe
        } else {
            first.or(fallback).or(probe)
        };
        let result = chosen.map(|(index, node)| {
            let offered = candidates.remove(index);
            candidates.push(offered);
            node
        });
        // Rotate even on a later ordinary refusal. Snapshot and live state
        // must agree before another worker or discovery step uses this queue.
        s.home.resources.candidates[kind as usize] = candidates;
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.resources.candidates[kind as usize] =
                s.home.resources.candidates[kind as usize].clone();
        }
        result
    }
}

fn block_offset(index: usize) -> (i32, i32) {
    if index == 0 {
        return (0, 0);
    }
    let ring = index.isqrt().div_ceil(2);
    let offset = index - (2 * ring - 1).pow(2);
    let side = 2 * ring;
    let at = (offset % side) as i32;
    let r = ring as i32;
    match offset / side {
        0 => (-r + at, -r),
        1 => (r, -r + at),
        2 => (r - at, r),
        _ => (-r, r - at),
    }
}
