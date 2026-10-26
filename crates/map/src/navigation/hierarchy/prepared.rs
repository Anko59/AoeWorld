//! Lazy exact land connectivity over chunk-local components. Preparation
//! captures collision only; no terrain is composed until a query visits it.
use super::*;
use std::collections::BTreeSet;
use std::sync::Mutex;
mod resolution;
pub(crate) use resolution::{CHUNK_WORK, Resolution, ResolutionOutcome};

type Component = (Key, u16);
type Query = (Component, TileCoord);

fn squared_distance(tile: TileCoord, destination: TileCoord) -> i64 {
    let (dx, dy) = (
        i64::from(tile.x) - i64::from(destination.x),
        i64::from(tile.y) - i64::from(destination.y),
    );
    dx * dx + dy * dy
}

fn closest_in_chunk(key: Key, destination: TileCoord) -> TileCoord {
    let low = grid::tile(key, 0);
    let high = grid::tile(key, grid::CELLS - 1);
    TileCoord::new(
        destination.x.clamp(low.x, high.x),
        destination.y.clamp(low.y, high.y),
    )
}

#[derive(Debug)]
pub(crate) struct Atlas {
    side: i32,
    overlay: ResourceOverlay,
    destinations: Mutex<BTreeMap<Query, TileCoord>>,
}

impl PartialEq for Atlas {
    fn eq(&self, other: &Self) -> bool {
        self.side == other.side && self.stamp() == other.stamp()
    }
}
impl Eq for Atlas {}

impl Atlas {
    pub(super) fn entry_count(&self) -> Option<usize> {
        self.destinations.lock().ok().map(|cache| cache.len())
    }

    fn contains(&self, key: Key) -> bool {
        key.0 >= 0 && key.1 >= 0 && key.0 < self.side && key.1 < self.side
    }
    pub(super) fn overlay(&self) -> &ResourceOverlay {
        &self.overlay
    }
    pub(super) fn stamp(&self) -> [u8; 32] {
        self.overlay.navigation_stamp()
    }

    pub(super) fn chunk(
        &self,
        generator: &MapChunkGenerator,
        key: Key,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<ChunkPortals>> {
        // A pinned search always uses the captured collision snapshot, even
        // when a tree is depleted between polls. Never hold a cache lock while
        // composing terrain or constructing the local graph.
        let ground = Ground::new(generator, &self.overlay, cancelled);
        ground.portals(key)
    }

    fn remember(
        &self,
        parents: &BTreeMap<Component, Option<Component>>,
        mut at: Option<Component>,
        destination: TileCoord,
        endpoint: TileCoord,
    ) {
        if let Ok(mut cache) = self.destinations.lock() {
            while let Some(component) = at {
                cache.insert((component, destination), endpoint);
                at = parents.get(&component).copied().flatten();
            }
            while cache.len() > MAX_CACHED_CHUNKS {
                cache.pop_first();
            }
        }
    }

    fn nearest(
        self: &Arc<Self>,
        generator: &MapChunkGenerator,
        origin: TileCoord,
        destination: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<TileCoord>> {
        let key = chunk_of(origin);
        if !self.contains(key) {
            return Ok(None);
        }
        let chunk = self.chunk(generator, key, cancelled)?;
        let label = chunk.labels[grid::index(origin)];
        if label == 0 {
            return Ok(None);
        }
        if let Some(known) = self
            .destinations
            .lock()
            .ok()
            .and_then(|cache| cache.get(&((key, label), destination)).copied())
        {
            return Ok(Some(known));
        }
        let mut resolution = Resolution::new(Arc::clone(self), origin, destination);
        loop {
            match resolution.poll(generator, &mut 65_536, None, cancelled)? {
                Some(ResolutionOutcome::Destination(target)) => return Ok(target),
                Some(ResolutionOutcome::Bounded) => return Err(EnvironmentPageError::Unavailable),
                None => {}
            }
        }
    }
}

impl MapChunkGenerator {
    pub(crate) fn has_prepared_navigation(&self, overlay: &ResourceOverlay) -> bool {
        self.prepared_navigation(overlay).is_some()
    }

    pub(super) fn prepared_navigation(&self, overlay: &ResourceOverlay) -> Option<Arc<Atlas>> {
        self.portal_cache()
            .lock()
            .ok()
            .and_then(|cache| cache.prepared.clone())
            .filter(|atlas| atlas.stamp() == overlay.navigation_stamp())
    }

    /// Capture collision for lazy navigation. Startup never composes terrain,
    /// floods chunks, or scans the map. Spatial fingerprints reuse unchanged
    /// local graphs on later snapshots; retained searches pin their old view.
    pub fn prepare_navigation(
        &self,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.has_prepared_navigation(overlay) {
            return Ok(());
        }
        let side = self.navigation_chunk_side();
        if side <= 0 {
            return Err(EnvironmentPageError::Invalid);
        }
        let atlas = Arc::new(Atlas {
            side,
            overlay: overlay.navigation_snapshot(),
            destinations: Mutex::default(),
        });
        if let Ok(mut cache) = self.portal_cache().lock() {
            cache.prepared = Some(atlas);
        }
        Ok(())
    }

    pub(crate) fn refresh_navigation(
        &self,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool> {
        let present = self
            .portal_cache()
            .lock()
            .ok()
            .is_some_and(|cache| cache.prepared.is_some());
        if present {
            self.prepare_navigation(overlay, cancelled)?;
        }
        Ok(present)
    }

    pub(crate) fn navigation_resolution(
        &self,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        destination: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Resolution>> {
        if self
            .portal_cache()
            .lock()
            .ok()
            .is_none_or(|cache| cache.prepared.is_none())
        {
            return Ok(None);
        }
        self.prepare_navigation(overlay, cancelled)?;
        Ok(self
            .prepared_navigation(overlay)
            .map(|atlas| Resolution::new(atlas, origin, destination)))
    }

    /// Exact nearest reachable tile (squared Euclidean distance, then y/x).
    /// Visits chunk components toward the destination, stopping as soon as
    /// connectivity is proved. Only a disconnected goal exhausts the reachable
    /// region. No inter-portal cost floods are needed for this query.
    pub fn navigation_destination(
        &self,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        destination: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<TileCoord>> {
        if self
            .portal_cache()
            .lock()
            .ok()
            .is_none_or(|cache| cache.prepared.is_none())
        {
            return Ok(None);
        }
        self.prepare_navigation(overlay, cancelled)?;
        self.prepared_navigation(overlay).map_or(Ok(None), |atlas| {
            atlas.nearest(self, origin, destination, cancelled)
        })
    }
}

#[cfg(test)]
mod tests;
