//! Adapter-owned spans. The map never reads a clock or branches on a sample.
use super::MapChunkGenerator;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationPhase {
    Terrain,
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
