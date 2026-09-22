use super::GameWorld;
use crate::Terrain;
use aoe_map::{Depletion, ResourceOverlay, ResourceOverlayError, ResourceOverlaySnapshot};

/// An exclusive, validated mutation. Dropping it preserves the live world.
/// The server can persist `snapshot()` before calling the infallible commit.
pub struct PreparedResourceDepletion<'a> {
    world: &'a mut GameWorld,
    overlay: ResourceOverlay,
    depletion: Depletion,
    previous_revision: u64,
}

impl PreparedResourceDepletion<'_> {
    pub fn previous_revision(&self) -> u64 {
        self.previous_revision
    }

    pub fn snapshot(&self, content_hash: [u8; 32]) -> ResourceOverlaySnapshot {
        self.overlay.snapshot(content_hash)
    }

    pub fn commit(self) -> Depletion {
        let Terrain::Map { overlay, .. } = &mut self.world.terrain else {
            unreachable!("prepared mutation exclusively borrows a map world");
        };
        *overlay = self.overlay;
        if self.depletion.became_nonblocking {
            self.world.resource_collision_changed();
        }
        self.depletion
    }
}

impl GameWorld {
    pub fn resource_snapshot(&self, content_hash: [u8; 32]) -> Option<ResourceOverlaySnapshot> {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return None;
        };
        Some(overlay.snapshot(content_hash))
    }

    /// Restore only before spawning units, so restored obstacles cannot cover
    /// an occupied position. The caller supplies the immutable package hash.
    pub fn restore_resources(
        &mut self,
        snapshot: &ResourceOverlaySnapshot,
        content_hash: [u8; 32],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), ResourceOverlayError> {
        if !self.units.is_empty() {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        let Terrain::Map { generator, overlay } = &mut self.terrain else {
            return Err(ResourceOverlayError::WrongMap);
        };
        let replacement =
            ResourceOverlay::from_snapshot(snapshot, content_hash, generator, cancelled)?;
        *overlay = replacement;
        self.navigation_cache.clear();
        Ok(())
    }

    pub fn prepare_resource_depletion(
        &mut self,
        id: u64,
        requested: u16,
    ) -> Result<PreparedResourceDepletion<'_>, ResourceOverlayError> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(ResourceOverlayError::UnknownResource);
        };
        let previous_revision = overlay.revision();
        let mut replacement = overlay.clone();
        let depletion = replacement.deplete(generator, id, requested)?;
        Ok(PreparedResourceDepletion {
            world: self,
            overlay: replacement,
            depletion,
            previous_revision,
        })
    }

    pub(super) fn resource_collision_changed(&mut self) {
        self.navigation_cache.clear();
        for index in 0..self.units.len() {
            self.clear_planner(index);
            self.units[index].last_movement_error = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::{MapPackage, MapRequest};

    #[test]
    fn dropped_mutation_and_failed_restore_preserve_world_then_commit_roundtrips() {
        let package = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
        let hash = package.content_hash;
        let node = (0..16)
            .find_map(|y| {
                (0..16).find_map(|x| {
                    package
                        .generator()
                        .chunk(x, y)
                        .ok()?
                        .resources
                        .into_iter()
                        .next()
                })
            })
            .expect("fixture resource");
        let mut world = GameWorld::from_map(package.clone()).unwrap();
        let before = world.canonical_hash();
        {
            let transaction = world.prepare_resource_depletion(node.id, 1).unwrap();
            assert_eq!(transaction.snapshot(hash).revision, 1);
        }
        assert_eq!(world.canonical_hash(), before);
        let change = world
            .prepare_resource_depletion(node.id, 1)
            .unwrap()
            .commit();
        assert_eq!(change.removed, 1);
        let snapshot = world.resource_snapshot(hash).unwrap();
        let mut restored = GameWorld::from_map(package).unwrap();
        assert!(
            restored
                .restore_resources(&snapshot, [9; 32], &|| false)
                .is_err()
        );
        assert_eq!(restored.canonical_hash(), before);
        restored
            .restore_resources(&snapshot, hash, &|| false)
            .unwrap();
        assert_eq!(restored.canonical_hash(), world.canonical_hash());
    }
}
