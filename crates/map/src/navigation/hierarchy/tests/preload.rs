use super::*;
use crate::{NavigationPhase, NavigationProbe, NavigationSpan, ResourceOverlay};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct Probe(AtomicUsize);
struct Span;
impl NavigationSpan for Span {}
impl NavigationProbe for Probe {
    fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
        if phase == NavigationPhase::TerrainPoint {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        Box::new(Span)
    }
}

#[test]
fn cold_portals_reuse_their_chunk_projection_for_border_points() {
    let generator = flat_terrain_with_water_wall(64, -1, -1);
    let probe = Arc::new(Probe::default());
    generator.set_navigation_probe(probe.clone());
    let ground = Ground::new(&generator, ResourceOverlay::empty(), &|| false);
    let graph = ground.build_portals((0, 0)).unwrap();
    assert_eq!(graph.components(), 1);
    assert!(graph.portal_count() > 0);
    assert!(
        probe.0.load(Ordering::Relaxed) <= 2 * CHUNK_TILES as usize + 8,
        "border points inside the chunk must reuse its already-required projection: {}",
        probe.0.load(Ordering::Relaxed)
    );
}
