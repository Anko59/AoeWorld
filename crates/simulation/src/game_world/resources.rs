use super::GameWorld;
use crate::Terrain;
use aoe_map::{Depletion, ResourceOverlay, ResourceOverlayError, ResourceOverlaySnapshot};

/// An exclusive, validated mutation. Dropping it preserves the live world.
/// The server can persist `snapshot()` before calling the infallible commit.
pub struct PreparedResourceDepletion<'a> {
    world: &'a mut GameWorld,
    depletion: Depletion,
    previous_revision: u64,
}

impl PreparedResourceDepletion<'_> {
    pub fn previous_revision(&self) -> u64 {
        self.previous_revision
    }

    pub fn snapshot(&self, content_hash: [u8; 32]) -> ResourceOverlaySnapshot {
        // Built on demand: most consumers never persist, and cloning the
        // overlay for each harvest would cost O(touched resources) per hit.
        let Terrain::Map { overlay, .. } = &self.world.terrain else {
            unreachable!("prepared mutation exclusively borrows a map world");
        };
        let mut next = overlay.clone();
        next.apply_depletion(&self.depletion);
        next.snapshot(content_hash)
    }

    pub fn commit(self) -> Depletion {
        let Terrain::Map { overlay, .. } = &mut self.world.terrain else {
            unreachable!("prepared mutation exclusively borrows a map world");
        };
        overlay.apply_depletion(&self.depletion);
        if self.depletion.became_nonblocking {
            self.world
                .collision_opened(resource_area(self.depletion.id));
        }
        self.world
            .record_observed_resource(self.depletion.id, self.depletion.remaining);
        self.depletion
    }
}

impl GameWorld {
    pub fn resource_revision(&self) -> Option<u64> {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return None;
        };
        Some(overlay.revision())
    }

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
        if !self.units.is_empty() || !self.economy.buildings.is_empty() {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        let Terrain::Map { generator, overlay } = &mut self.terrain else {
            return Err(ResourceOverlayError::WrongMap);
        };
        let replacement =
            ResourceOverlay::from_snapshot(snapshot, content_hash, generator, cancelled)?;
        *overlay = replacement;
        if let Ok(cache) = self.resource_chunks.get_mut() {
            cache.clear();
        }
        self.navigation_cache.clear();
        self.economy.work_positions.clear();
        self.clearance
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
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
        let depletion = overlay.plan_depletion(generator, id, requested)?;
        Ok(PreparedResourceDepletion {
            world: self,
            depletion,
            previous_revision,
        })
    }

    pub(super) fn resource_collision_changed(&mut self) {
        self.navigation_cache.clear();
        self.economy.work_positions.clear();
        self.clearance
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        for index in 0..self.units.len() {
            self.clear_planner(index);
            self.units[index].last_movement_error = None;
        }
        self.collision_subscribers.errors.clear();
    }

    /// Ground became walkable (a resource exhausted, a building removed, a
    /// gate opened). Keep planned routes and searches unless the opening
    /// can invalidate a suspended reverse flood's sealed-destination proof.
    /// Restart those planners through the scheduler's normal budget; clearing
    /// every planner whenever a tree fell caused the long-game freeze.
    /// Only the subscribed units are visited, never every unit.
    pub(crate) fn collision_opened(&mut self, area: aoe_core::TileRect) {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|probe| probe.cache_span());
        self.invalidate_collision_caches(area);
        self.clear_planners_where(|planner| planner.opening_invalidates_proof(area));
        self.clear_movement_errors();
        // Opened ground can make a refused target reachable.
        for id in std::mem::take(&mut self.collision_subscribers.memos) {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].unreachable_memo = None;
            }
        }
        #[cfg(test)]
        self.assert_collision_subscribers_complete(true);
    }

    /// `area` became blocked (a building placed, a gate closed): only the
    /// planners whose search or planned route touches it are planned again.
    pub(crate) fn collision_closed(&mut self, area: aoe_core::TileRect) {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|probe| probe.cache_span());
        self.invalidate_collision_caches(area);
        self.clear_planners_where(|planner| planner.touches(area));
        self.clear_movement_errors();
        #[cfg(test)]
        self.assert_collision_subscribers_complete(false);
    }

    fn invalidate_collision_caches(&mut self, area: aoe_core::TileRect) {
        self.navigation_cache.invalidate(area);
        self.economy.work_positions.invalidate(area);
        self.clearance
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .invalidate(area);
    }

    /// Clears the planners `invalid` selects, in unit order as the full
    /// scan did, visiting only the units that hold one.
    fn clear_planners_where(&mut self, invalid: impl Fn(&aoe_map::RoutePlanner) -> bool) {
        let (mut cleared, mut gone) = (Vec::new(), Vec::new());
        for id in &self.collision_subscribers.planners {
            match self.lookup.get(id) {
                Some(&index) if self.units[index].planner.as_ref().is_some_and(&invalid) => {
                    cleared.push(index);
                }
                Some(_) => {}
                None => gone.push(*id),
            }
        }
        for id in gone {
            self.collision_subscribers.planners.remove(&id);
        }
        cleared.sort_unstable();
        for index in cleared {
            self.clear_planner(index);
        }
    }

    fn clear_movement_errors(&mut self) {
        for id in std::mem::take(&mut self.collision_subscribers.errors) {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].last_movement_error = None;
            }
        }
    }

    /// Sets a unit's movement error; an error subscribes it to the next
    /// collision change, which clears it.
    pub(crate) fn set_movement_error(
        &mut self,
        index: usize,
        error: Option<crate::GameWorldError>,
    ) {
        self.units[index].last_movement_error = error;
        if error.is_some() {
            let id = self.units[index].state.id;
            self.collision_subscribers.errors.insert(id);
        }
    }

    /// Test oracle: the full unit scan this replaced would change nothing more.
    #[cfg(test)]
    fn assert_collision_subscribers_complete(&self, opened: bool) {
        for unit in &self.units {
            let id = unit.state.id;
            assert!(unit.last_movement_error.is_none(), "{id:?} kept its error");
            assert!(
                !opened || unit.unreachable_memo.is_none(),
                "{id:?} kept its memo"
            );
            assert_eq!(
                unit.planner.is_some(),
                self.collision_subscribers.planners.contains(&id),
                "{id:?} planner subscription"
            );
        }
    }
}

/// The units a collision change can affect: planner holders (their search
/// or route may cross the area), and the units holding a movement error or
/// an unreachable memo, which every change clears. Ids are monotonic, so a
/// removed unit's stale id is skipped by the lookup.
#[derive(Debug, Default)]
pub(crate) struct CollisionSubscribers {
    pub(crate) planners: std::collections::BTreeSet<aoe_core::EntityId>,
    pub(crate) errors: std::collections::BTreeSet<aoe_core::EntityId>,
    pub(crate) memos: std::collections::BTreeSet<aoe_core::EntityId>,
}

/// The ground a resource node blocks: its tile (natural and starting node
/// identities both encode it), with a tile of margin.
pub(crate) fn resource_area(id: u64) -> aoe_core::TileRect {
    let x = ((id >> 1) & 0x3_ffff) as i32;
    let y = ((id >> 19) & 0x3_ffff) as i32;
    aoe_core::TileRect::new(
        aoe_core::TileCoord::new(x - 1, y - 1),
        aoe_core::TileCoord::new(x + 2, y + 2),
    )
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
