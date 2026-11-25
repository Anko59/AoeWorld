//! Rasters that run at the same time: fine ones share four threads, a coarse
//! one has its own. Tests hold workers inside a page load through channels;
//! nothing sleeps and every wait is bounded.
use super::super::super::minimap::minimap_raster_at_samples;
use super::pages::{
    AXIS, COARSE, FINE, PageSource, TILES, UNLIMITED, WORKERS, generator_over, wide_environment,
    wide_pages, wide_raster,
};
use super::reference::{raster_at, reference};
use super::sources::country;
use super::*;
use aoe_map::{EnvironmentPageKey, EnvironmentPageProvider};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

/// Only a failing test waits this long.
const WAIT: Duration = Duration::from_secs(30);

/// Reports every call, and holds it until the gate is open.
#[derive(Debug)]
struct Gate {
    source: Arc<PageSource>,
    calls: Sender<()>,
    /// One sender per held call, dropped to release it; `None` once open.
    held: Mutex<Option<Vec<Sender<()>>>>,
}

impl Gate {
    fn closed(calls: Sender<()>) -> Arc<Self> {
        Arc::new(Self {
            source: PageSource::new(wide_pages(), UNLIMITED),
            calls,
            held: Mutex::new(Some(Vec::new())),
        })
    }

    fn open(&self) {
        if let Ok(mut held) = self.held.lock() {
            held.take();
        }
    }
}

impl EnvironmentPageProvider for Gate {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        let held = self.held.lock().expect("gate").as_mut().map(|held| {
            let (release, released) = mpsc::channel::<()>();
            held.push(release);
            released
        });
        // A test that has ended no longer listens.
        let _ = self.calls.send(());
        if let Some(released) = held {
            // Longer than the test's own wait, so the test gives up first.
            let _ = released.recv_timeout(WAIT * 2);
        }
        self.source.page(key, cancelled)
    }
}

/// Ends every raster of a test, also when an assertion fails.
struct Release<'a> {
    gates: Vec<&'a Gate>,
    stop: &'a AtomicBool,
}

impl Drop for Release<'_> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for gate in &self.gates {
            gate.open();
        }
    }
}

fn gated(gate: &Arc<Gate>) -> MapChunkGenerator {
    generator_over(gate.clone(), TILES, wide_environment())
}

fn stoppable(
    generator: &MapChunkGenerator,
    samples: u16,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    minimap_raster_at_samples(generator, TILES, 60_000, AXIS, samples, stopped)
}

struct Job<'a> {
    name: &'static str,
    generator: &'a MapChunkGenerator,
    samples: u16,
    expected: Vec<u8>,
}

impl<'a> Job<'a> {
    fn new(name: &'static str, generator: &'a MapChunkGenerator, samples: u16) -> Self {
        Self {
            name,
            generator,
            samples,
            expected: reference(generator, 256, 16, samples).raster,
        }
    }

    fn raster(&self) -> Vec<u8> {
        raster_at(self.generator, 256, 16, self.samples)
    }
}

/// The maps share geography and tiles and differ only in land use, so one
/// map's thread-local tables would give the other wrong wildness, forests and
/// classes wherever a thread took one raster's tables for the other's.
#[test]
fn rasters_of_two_maps_at_once_give_their_own_reference_bytes() {
    let (open, farmed) = (country(256, false), country(256, true));
    let jobs = [
        Job::new("open fine", &open, 128),
        Job::new("farmed fine", &farmed, 128),
        Job::new("open coarse", &open, COARSE),
        Job::new("farmed coarse", &farmed, COARSE),
    ];
    assert!(count(&jobs[0].expected, MinimapClass::Forest) > 0);
    assert_ne!(jobs[0].expected, jobs[1].expected);
    for _ in 0..3 {
        // Fine and fine, fine and coarse, coarse and coarse.
        for (first, second) in [(0, 1), (1, 0), (0, 3), (1, 2), (2, 3)] {
            let (first, second) = (&jobs[first], &jobs[second]);
            thread::scope(|scope| {
                let pair = [first, second].map(|job| (job, scope.spawn(move || job.raster())));
                for (job, raster) in pair {
                    let same = raster.join().expect("raster thread") == job.expected;
                    assert!(same, "{} of {} and {}", job.name, first.name, second.name);
                }
            });
        }
    }
}

#[test]
fn a_coarse_raster_completes_while_every_fine_worker_waits_for_a_page() {
    let (calls, entered) = mpsc::channel();
    let gate = Gate::closed(calls);
    let fine = gated(&gate);
    let free = PageSource::new(wide_pages(), UNLIMITED);
    let stop = AtomicBool::new(false);
    let (done, finished) = mpsc::channel();
    thread::scope(|scope| {
        let _release = Release {
            gates: vec![&*gate],
            stop: &stop,
        };
        let (fine, free, stop) = (&fine, &free, &stop);
        let stopped = move || stop.load(Ordering::Relaxed);
        scope.spawn(move || stoppable(fine, FINE, &stopped));
        for worker in 0..WORKERS {
            assert!(
                entered.recv_timeout(WAIT).is_ok(),
                "only {worker} fine workers reached a page"
            );
        }
        scope.spawn(move || {
            let raster = wide_raster(free, COARSE, &stopped);
            let _ = done.send(raster.map(|raster| raster.len()));
        });
        let length = MINIMAP_HEADER_BYTES + usize::from(COARSE).pow(2);
        assert_eq!(
            finished.recv_timeout(WAIT),
            Ok(Ok(length)),
            "a coarse raster waits for a fine one"
        );
        assert!(entered.try_recv().is_err(), "the fine workers still wait");
    });
}

/// The later raster's own source stays closed. A worker of the first that took
/// the later raster's task would hang there and could not answer its own
/// cancellation either.
#[test]
fn a_started_fine_raster_is_not_paused_by_a_later_one() {
    let counted = PageSource::new(wide_pages(), UNLIMITED);
    wide_raster(&counted, FINE, &|| false).expect("calibration");
    let quarter = counted.calls() as usize / 4;
    let (calls, first_pages) = mpsc::channel();
    let (later_calls, later_pages) = mpsc::channel();
    let (first_gate, later_gate) = (Gate::closed(calls), Gate::closed(later_calls));
    let (first, later) = (gated(&first_gate), gated(&later_gate));
    let (stop, cancel_first) = (AtomicBool::new(false), AtomicBool::new(false));
    let (done, finished) = mpsc::channel();
    let (announce, announced) = mpsc::channel();
    thread::scope(|scope| {
        let _release = Release {
            gates: vec![&*first_gate, &*later_gate],
            stop: &stop,
        };
        let (first, later) = (&first, &later);
        let (stop, cancel_first) = (&stop, &cancel_first);
        scope.spawn(move || {
            let stopped = || stop.load(Ordering::Relaxed) || cancel_first.load(Ordering::Relaxed);
            let _ = done.send(stoppable(first, FINE, &stopped));
        });
        for worker in 0..WORKERS {
            assert!(
                first_pages.recv_timeout(WAIT).is_ok(),
                "only {worker} workers of the first raster reached a page"
            );
        }
        scope.spawn(move || {
            let _ = announce.send(());
            let _ = stoppable(later, FINE, &|| stop.load(Ordering::Relaxed));
        });
        assert!(announced.recv_timeout(WAIT).is_ok());
        first_gate.open();
        for call in WORKERS..quarter {
            assert!(
                first_pages.recv_timeout(WAIT).is_ok(),
                "the first raster stopped at {call} of {quarter} calls: a later one paused it"
            );
        }
        assert!(
            later_pages.try_recv().is_err(),
            "the later raster ran on a thread of the first"
        );
        cancel_first.store(true, Ordering::Relaxed);
        assert_eq!(
            finished.recv_timeout(WAIT),
            Ok(Err(MinimapError::Page(EnvironmentPageError::Cancelled)))
        );
    });
}
