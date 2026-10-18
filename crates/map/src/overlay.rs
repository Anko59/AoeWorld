use crate::{MapChunkGenerator, ResourceKind, ResourceNode};
use aoe_core::{TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};

mod snapshot;
mod starting;
pub use snapshot::{RESOURCE_OVERLAY_SCHEMA_VERSION, ResourceChange, ResourceOverlaySnapshot};

/// Logical entry limit; allocator overhead is not included in this count.
pub const MAX_RESOURCE_OVERLAY_CHANGES: usize = 65_536;
/// Building collision tiles the overlay holds at once (towns, houses, walls).
pub const MAX_OBSTACLE_TILES: u64 = 4_096;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceOverlay {
    remaining: BTreeMap<u64, u16>,
    revision: u64,
    obstacles: BTreeSet<TileCoord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Depletion {
    pub id: u64,
    pub removed: u16,
    pub remaining: u16,
    pub became_nonblocking: bool,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ResourceOverlayError {
    #[error("resource does not exist in this map")]
    UnknownResource,
    #[error("resource overlay has reached its changed-resource limit")]
    Capacity,
    #[error("resource overlay revision is exhausted")]
    RevisionExhausted,
    #[error("resource overlay snapshot is invalid")]
    InvalidSnapshot,
    #[error("resource overlay belongs to a different immutable map")]
    WrongMap,
    #[error("resource terrain could not be read: {0}")]
    Environment(#[from] crate::EnvironmentPageError),
}

impl ResourceOverlay {
    /// Transient building collision, reconstructed by the simulation on start.
    /// It is separate from resource revisions and their persisted snapshots.
    pub fn add_footprint(&mut self, rect: TileRect) -> Result<(), ResourceOverlayError> {
        let area = u64::try_from(rect.width())
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(rect.height()).unwrap_or(u64::MAX));
        if rect.min.x < 0
            || rect.min.y < 0
            || area == 0
            || area > MAX_OBSTACLE_TILES
            || (self.obstacles.len() as u64).saturating_add(area) > MAX_OBSTACLE_TILES
        {
            return Err(ResourceOverlayError::Capacity);
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                self.obstacles.insert(TileCoord::new(x, y));
            }
        }
        Ok(())
    }

    /// Remove collision when a building is cancelled or destroyed.
    pub fn remove_footprint(&mut self, rect: TileRect) {
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                self.obstacles.remove(&TileCoord::new(x, y));
            }
        }
    }

    pub fn blocks_tile(&self, tile: TileCoord) -> bool {
        self.obstacles.contains(&tile)
            || (!self.remaining.is_empty()
                && [
                    crate::ResourceKind::Food,
                    crate::ResourceKind::Wood,
                    crate::ResourceKind::Gold,
                    crate::ResourceKind::Stone,
                ]
                .into_iter()
                .filter_map(|kind| ResourceNode::starting_id(tile, kind))
                .any(|id| self.remaining.get(&id).is_some_and(|amount| *amount > 0)))
    }

    /// Startup insertion is part of the initial snapshot, not a depletion revision.
    pub fn insert_starting_resource(
        &mut self,
        terrain: &MapChunkGenerator,
        node: ResourceNode,
    ) -> Result<(), ResourceOverlayError> {
        if self.starting_nodes().count() >= 512
            || self.remaining.len() >= MAX_RESOURCE_OVERLAY_CHANGES
        {
            return Err(ResourceOverlayError::Capacity);
        }
        if ResourceNode::starting_from_id(node.id) != Some(node)
            || self.remaining.contains_key(&node.id)
            || self.blocks_tile(node.tile)
            || !terrain
                .tile_and_node_with_cancel(node.tile, &|| false)?
                .is_some_and(|(tile, object)| tile.passable && object.is_none())
        {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        self.remaining.insert(node.id, node.initial_amount);
        Ok(())
    }

    pub fn starting_nodes(&self) -> impl Iterator<Item = ResourceNode> + '_ {
        self.remaining
            .range((1 << 37)..)
            .filter_map(|(id, _)| ResourceNode::starting_from_id(*id))
    }

    /// Starting resources whose tiles lie in the inclusive rectangle, in the
    /// same identity order as `starting_nodes`. Identities are kind-major and
    /// then row-major, so each (kind, row) is one contiguous range.
    pub fn starting_nodes_in(
        &self,
        min: TileCoord,
        max: TileCoord,
    ) -> impl Iterator<Item = ResourceNode> + '_ {
        const LIMIT: i32 = 1 << 18;
        let (x0, x1) = (min.x.max(0), max.x.min(LIMIT - 1));
        let (y0, y1) = (min.y.max(0), max.y.min(LIMIT - 1));
        let rows = (y0..=y1).filter(move |_| x0 <= x1);
        [
            ResourceKind::Food,
            ResourceKind::Wood,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ]
        .into_iter()
        .flat_map(move |kind| {
            rows.clone().flat_map(move |y| {
                let first = ResourceNode::starting_id(TileCoord::new(x0, y), kind);
                let last = ResourceNode::starting_id(TileCoord::new(x1, y), kind);
                first.zip(last).into_iter().flat_map(move |(first, last)| {
                    self.remaining
                        .range(first..=last)
                        .filter_map(|(id, _)| ResourceNode::starting_from_id(*id))
                })
            })
        })
    }

    pub fn resource_by_id_with_cancel(
        &self,
        terrain: &MapChunkGenerator,
        id: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResourceNode>, crate::EnvironmentPageError> {
        if cancelled() {
            return Err(crate::EnvironmentPageError::Cancelled);
        }
        if let Some(node) = ResourceNode::starting_from_id(id) {
            return Ok(self.remaining.contains_key(&id).then_some(node));
        }
        terrain.resource_by_id_with_cancel(id, cancelled)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn remaining(&self, terrain: &MapChunkGenerator, id: u64) -> Option<u16> {
        self.resource_by_id_with_cancel(terrain, id, &|| false)
            .ok()
            .flatten()
            .map(|node| {
                self.remaining
                    .get(&id)
                    .copied()
                    .unwrap_or(node.initial_amount)
            })
    }

    pub fn blocks(&self, terrain: &MapChunkGenerator, id: u64) -> bool {
        self.remaining(terrain, id).is_some_and(|amount| amount > 0)
    }

    /// Applies overlay state to a resource that was already resolved by a
    /// checked terrain query. This avoids resolving a provider-backed tile a
    /// second time after its page handle has been validated.
    pub fn blocks_node(&self, node: ResourceNode) -> bool {
        self.remaining
            .get(&node.id)
            .copied()
            .unwrap_or(node.initial_amount)
            > 0
    }

    pub fn deplete(
        &mut self,
        terrain: &MapChunkGenerator,
        id: u64,
        requested: u16,
    ) -> Result<Depletion, ResourceOverlayError> {
        let node = self
            .resource_by_id_with_cancel(terrain, id, &|| false)?
            .ok_or(ResourceOverlayError::UnknownResource)?;
        let previous = self
            .remaining
            .get(&id)
            .copied()
            .unwrap_or(node.initial_amount);
        let removed = previous.min(requested);
        let remaining = previous - removed;
        let became_nonblocking = previous > 0 && remaining == 0;
        if removed > 0 {
            if !self.remaining.contains_key(&id)
                && self.remaining.len() >= MAX_RESOURCE_OVERLAY_CHANGES
            {
                return Err(ResourceOverlayError::Capacity);
            }
            let revision = self
                .revision
                .checked_add(1)
                .ok_or(ResourceOverlayError::RevisionExhausted)?;
            self.remaining.insert(id, remaining);
            self.revision = revision;
        }
        Ok(Depletion {
            id,
            removed,
            remaining,
            became_nonblocking,
            revision: self.revision,
        })
    }

    pub fn changed(&self) -> impl Iterator<Item = (u64, u16)> + '_ {
        self.remaining.iter().map(|(id, amount)| (*id, *amount))
    }

    pub fn changed_count(&self) -> usize {
        self.remaining.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResourceNode;

    pub(super) fn resource() -> (MapChunkGenerator, ResourceNode) {
        let terrain = MapChunkGenerator::new([3; 32], 1, 128);
        let node = (0..4)
            .flat_map(|y| {
                let terrain = terrain.clone();
                (0..4).flat_map(move |x| terrain.chunk(x, y).expect("fixture chunk").resources)
            })
            .next()
            .expect("test terrain resource");
        (terrain, node)
    }

    #[test]
    fn depletion_saturates_and_changes_collision_once() {
        let (terrain, node) = resource();
        assert_eq!(terrain.resource_by_id(node.id), Some(node));
        let mut overlay = ResourceOverlay::default();
        let first = overlay
            .deplete(&terrain, node.id, node.initial_amount - 1)
            .expect("resource");
        assert_eq!(first.remaining, 1);
        assert!(!first.became_nonblocking);
        let final_depletion = overlay.deplete(&terrain, node.id, 100).expect("resource");
        assert_eq!(final_depletion.removed, 1);
        assert!(final_depletion.became_nonblocking);
        assert!(!overlay.blocks(&terrain, node.id));
        let exhausted = overlay.deplete(&terrain, node.id, 1).expect("resource");
        assert_eq!(exhausted.removed, 0);
        assert!(!exhausted.became_nonblocking);
        assert_eq!(overlay.revision(), 2);
    }

    #[test]
    fn invalid_resources_cannot_create_overlay_state() {
        let (terrain, _) = resource();
        let mut overlay = ResourceOverlay::default();
        assert_eq!(
            overlay.deplete(&terrain, u64::MAX, 1),
            Err(ResourceOverlayError::UnknownResource)
        );
        assert_eq!(overlay.revision(), 0);
    }
}
