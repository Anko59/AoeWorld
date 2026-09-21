use crate::{GameWorld, NavigationCacheUsage};

impl GameWorld {
    pub fn navigation_cache_usage(&self) -> NavigationCacheUsage {
        self.navigation_cache.usage()
    }
}
