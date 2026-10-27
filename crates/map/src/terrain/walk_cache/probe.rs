//! Adapter-owned spans. The map never reads a clock or branches on a sample.
use super::MapChunkGenerator;
use std::sync::Arc;

/// A background installer must not keep a retired world's caches alive.
#[derive(Clone, Debug)]
pub struct NavigationTarget {
    owner: std::sync::Weak<super::DerivedCaches>,
    side: i32,
}
impl NavigationTarget {
    pub fn is_alive(&self) -> bool {
        self.owner.strong_count() > 0
    }
    pub fn install(
        &self,
        snapshot: Arc<crate::NavigationSnapshot>,
    ) -> Result<(), crate::EnvironmentPageError> {
        if snapshot.chunk_side() != self.side {
            return Err(crate::EnvironmentPageError::Invalid);
        }
        if let Some(owner) = self.owner.upgrade() {
            let _ = owner.static_navigation.set(snapshot);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationPhase {
    Terrain,
    TerrainPoint,
    WalkCache,
    Portals,
    PortalCosts,
    Components,
    Hierarchy,
    Pocket,
}

impl NavigationPhase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Terrain => "terrain",
            Self::TerrainPoint => "terrain_point",
            Self::WalkCache => "walk_cache",
            Self::Portals => "portals",
            Self::PortalCosts => "portal_costs",
            Self::Components => "components",
            Self::Hierarchy => "hierarchy",
            Self::Pocket => "pocket",
        }
    }
}

/// A measurement ends on drop, including error and cancellation returns.
pub trait NavigationSpan {}

pub trait NavigationProbe: std::fmt::Debug + Send + Sync {
    fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan>;
}

impl MapChunkGenerator {
    pub fn navigation_target(&self) -> NavigationTarget {
        NavigationTarget {
            owner: Arc::downgrade(&self.derived),
            side: self.navigation_chunk_side(),
        }
    }
    pub fn set_navigation_probe(&self, probe: Arc<dyn NavigationProbe>) {
        if let Ok(mut slot) = self.derived.probe.write() {
            *slot = Some(probe);
        }
    }

    pub(crate) fn measure_navigation(
        &self,
        phase: NavigationPhase,
    ) -> Option<Box<dyn NavigationSpan>> {
        let probe = self.derived.probe.read().ok()?.clone()?;
        Some(probe.begin(phase))
    }
}
