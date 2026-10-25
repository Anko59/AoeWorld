use crate::{GameWorld, NavigationCacheUsage};
mod resources;
pub(crate) use resources::ResourceCache;

impl GameWorld {
    pub fn navigation_cache_usage(&self) -> NavigationCacheUsage {
        self.navigation_cache.usage()
    }

    /// Retained candidate blocks and allocation capacities, excluding allocator slack.
    pub fn resource_cache_usage(&self) -> (usize, usize) {
        self.resource_chunks
            .lock()
            .map_or((0, 0), |cache| cache.usage())
    }
}

#[cfg(test)]
mod tests;
