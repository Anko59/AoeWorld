use crate::MapChunkGenerator;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceOverlay {
    remaining: BTreeMap<u64, u16>,
    revision: u64,
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
}

impl ResourceOverlay {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn remaining(&self, terrain: &MapChunkGenerator, id: u64) -> Option<u16> {
        terrain.resource_by_id(id).map(|node| {
            self.remaining
                .get(&id)
                .copied()
                .unwrap_or(node.initial_amount)
        })
    }

    pub fn blocks(&self, terrain: &MapChunkGenerator, id: u64) -> bool {
        self.remaining(terrain, id).is_some_and(|amount| amount > 0)
    }

    pub fn deplete(
        &mut self,
        terrain: &MapChunkGenerator,
        id: u64,
        requested: u16,
    ) -> Result<Depletion, ResourceOverlayError> {
        let node = terrain
            .resource_by_id(id)
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
            self.remaining.insert(id, remaining);
            self.revision = self.revision.saturating_add(1);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResourceNode;

    fn resource() -> (MapChunkGenerator, ResourceNode) {
        let terrain = MapChunkGenerator::new([3; 32], 1, 128);
        let node = (0..4)
            .flat_map(|y| {
                let terrain = terrain.clone();
                (0..4).flat_map(move |x| terrain.chunk(x, y).resources)
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
