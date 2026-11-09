use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Default)]
struct Probe(Arc<[AtomicUsize; 4]>);

struct Span {
    counts: Arc<[AtomicUsize; 4]>,
    end: usize,
}

impl aoe_map::NavigationSpan for Span {}

impl Drop for Span {
    fn drop(&mut self) {
        self.counts[self.end].fetch_add(1, Ordering::Relaxed);
    }
}

impl crate::PlanningProbe for Probe {
    fn begin(&self) {}
    fn end(&self) {}

    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        let begin = match name {
            "index_collision" => 0,
            "index_chunks" => 2,
            _ => return None,
        };
        self.0[begin].fetch_add(1, Ordering::Relaxed);
        Some(Box::new(Span {
            counts: self.0.clone(),
            end: begin + 1,
        }))
    }
}

#[test]
fn relocation_spans_balance_without_changing_tick_output_or_replay() {
    let mut plain = world();
    let mut observed = world();
    let id = plain.spawn_unit(PlayerId(0), at(31, 50)).unwrap();
    assert_eq!(observed.spawn_unit(PlayerId(0), at(31, 50)).unwrap(), id);
    let probe = Arc::new(Probe::default());
    observed.set_planning_probe(probe.clone());
    plain.issue_move(id, at(35, 50)).unwrap();
    observed.issue_move(id, at(35, 50)).unwrap();
    for _ in 0..100 {
        assert_eq!(plain.advance(), observed.advance());
        assert_eq!(plain.canonical_hash(), observed.canonical_hash());
    }
    assert!(observed.unit(id).unwrap().position.tile_floor().x >= 32);
    for begin in [0, 2] {
        let starts = probe.0[begin].load(Ordering::Relaxed);
        assert!(starts > 0, "missing relocation span at counter {begin}");
        assert_eq!(starts, probe.0[begin + 1].load(Ordering::Relaxed));
    }
}
