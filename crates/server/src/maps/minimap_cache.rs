//! The rasters one server keeps in memory, and the turn to compute another.
use super::*;
use std::sync::{Mutex, MutexGuard, PoisonError};

type Entries = VecDeque<(String, Arc<[u8]>)>;

/// Most-recently-used rasters by package content hash. Clones share them. This
/// lock is taken last and never held across a wait.
#[derive(Clone, Debug, Default)]
struct Ready(Arc<Mutex<Entries>>);

impl Ready {
    fn entries(&self) -> MutexGuard<'_, Entries> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn get(&self, hash: &str) -> Option<Arc<[u8]>> {
        let mut entries = self.entries();
        let index = entries.iter().position(|(key, _)| key == hash)?;
        let entry = entries.remove(index)?;
        let raster = entry.1.clone();
        entries.push_back(entry);
        Some(raster)
    }

    fn insert(&self, hash: String, raster: Arc<[u8]>) {
        let mut entries = self.entries();
        entries.retain(|(key, _)| *key != hash);
        entries.push_back((hash, raster));
        while entries.len() > MAX_CACHED_MINIMAPS {
            entries.pop_front();
        }
    }
}

/// What a computation reads and changes while it has its turn.
#[derive(Debug, Default)]
pub(crate) struct MinimapCache {
    ready: Ready,
    /// Rasters still computing after a request hit its deadline.
    pub(in crate::maps) pending: BTreeSet<String>,
}

impl MinimapCache {
    pub(crate) fn get(&mut self, hash: &str) -> Option<Arc<[u8]>> {
        self.ready.get(hash)
    }

    pub(crate) fn insert(&mut self, hash: String, raster: Arc<[u8]>) {
        self.ready.insert(hash, raster);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.ready.entries().len()
    }
}

/// Every raster of one server, ready or computing.
#[derive(Clone, Debug)]
pub(crate) struct Minimaps {
    /// The entries of the cache below, read without its lock.
    ready: Ready,
    /// Held while a raster is computed, so one is computed at a time.
    pub(in crate::maps) computing: Arc<tokio::sync::Mutex<MinimapCache>>,
}

impl Default for Minimaps {
    fn default() -> Self {
        let cache = MinimapCache::default();
        Self {
            ready: cache.ready.clone(),
            computing: Arc::new(tokio::sync::Mutex::new(cache)),
        }
    }
}

impl Minimaps {
    /// Waits for the turn to compute a raster.
    pub(in crate::maps) async fn lock(&self) -> tokio::sync::MutexGuard<'_, MinimapCache> {
        self.computing.lock().await
    }

    /// A raster from memory, whatever is computed now.
    pub(in crate::maps) fn ready(&self, key: &str) -> Option<Arc<[u8]>> {
        self.ready.get(key)
    }
}
