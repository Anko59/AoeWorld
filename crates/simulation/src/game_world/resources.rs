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
        if self.depletion.remaining == 0 {
            self.world
                .economy
                .gatherers
                .wake_target(crate::game_world::economy::WorkTarget::Node(
                    self.depletion.id,
                ));
        }
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
        for cache in &mut self.economy.movement_ground {
            cache.get_mut().unwrap_or_else(|e| e.into_inner()).clear();
        }
        self.economy
            .naval_ground
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
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
        for cache in &mut self.economy.movement_ground {
            cache.get_mut().unwrap_or_else(|e| e.into_inner()).clear();
        }
        self.economy
            .naval_ground
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.regions_changed_everywhere();
        self.economy.work_positions.clear();
        self.clearance
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        for index in 0..self.units.len() {
            self.clear_planner(index);
            self.set_movement_error(index, None);
        }
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
        let mut visited =
            self.clear_planners_where(|planner| planner.opening_invalidates_proof(area));
        visited += self.clear_movement_errors();
        // Opened ground can make a refused target reachable.
        let memos = std::mem::take(&mut self.collision_subscribers.memos);
        visited += memos.len();
        for id in memos {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].unreachable_memo = None;
            }
        }
        self.passes
            .record(crate::game_world::Pass::CollisionOpened, visited);
        #[cfg(test)]
        self.assert_collision_subscribers_complete(area, true);
    }

    /// `area` became blocked (a building placed, a gate closed): only the
    /// planners whose search or planned route touches it are planned again.
    pub(crate) fn collision_closed(&mut self, area: aoe_core::TileRect) {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|probe| probe.cache_span());
        self.invalidate_collision_caches(area);
        let visited = self.clear_planners_where(|planner| planner.touches(area))
            + self.clear_movement_errors();
        self.passes
            .record(crate::game_world::Pass::CollisionClosed, visited);
        #[cfg(test)]
        self.assert_collision_subscribers_complete(area, false);
    }

    fn invalidate_collision_caches(&mut self, area: aoe_core::TileRect) {
        for cache in &mut self.economy.movement_ground {
            cache
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .invalidate(area);
        }
        self.economy
            .naval_ground
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate(area);
        self.economy.gatherers.notify_walker_ground(area);
        self.wake_queue_ground(area);
        self.navigation_cache.invalidate(area);
        self.regions_changed(area);
        self.economy.work_positions.invalidate(area);
        self.clearance
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .invalidate(area);
    }

    /// Clears the planners `invalid` selects, in unit order as the full
    /// scan did, visiting only the units that hold one. Returns the visits.
    fn clear_planners_where(&mut self, invalid: impl Fn(&aoe_map::RoutePlanner) -> bool) -> usize {
        let visited = self.collision_subscribers.planners.len();
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
        visited
    }

    fn clear_movement_errors(&mut self) -> usize {
        let errors = std::mem::take(&mut self.collision_subscribers.errors);
        let visited = errors.len();
        for id in errors {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].last_movement_error = None;
            }
        }
        visited
    }

    /// Sets a unit's movement error; an error subscribes it to the next
    /// collision change, which clears it.
    pub(crate) fn set_movement_error(
        &mut self,
        index: usize,
        error: Option<crate::GameWorldError>,
    ) {
        self.units[index].last_movement_error = error;
        let id = self.units[index].state.id;
        if error.is_some() {
            self.economy.gatherers.wake_walking(id);
            self.collision_subscribers.errors.insert(id);
        } else {
            self.collision_subscribers.errors.remove(&id);
        }
    }

    /// Test oracle: the full unit scan this replaced would change nothing more.
    #[cfg(test)]
    fn assert_collision_subscribers_complete(&self, area: aoe_core::TileRect, opened: bool) {
        for unit in self.units.oracle_slice() {
            let id = unit.state.id;
            assert!(
                unit.planner.as_ref().is_none_or(|planner| {
                    if opened {
                        !planner.opening_invalidates_proof(area)
                    } else {
                        !planner.touches(area)
                    }
                }),
                "{id:?} kept an invalidated planner"
            );
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
/// an unreachable memo (cleared on openings). Entries leave with their state
/// or their unit, so storage is bounded by the live unit slots.
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
mod tests;
