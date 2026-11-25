//! What one raster asks of its page source: page loads behind a cache the size
//! of the server's, calls to the shared provider, landscape work per sample
//! and what still happens after a cancellation. Counts only, never time.
use super::super::super::minimap::minimap_raster_at_samples;
use super::sources::{relief_environment, relief_pages, relief_source};
use super::*;
use aoe_map::{
    EnvironmentPageKey, EnvironmentPageProvider, HydrologyEvidenceIndex, HydrologyEvidencePage,
    HydrologyKind, HydrologyWaterPolicy, ModernLandCoverPage, PageLayer,
    WORLD_COVER_OBSERVATION_YEAR,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// `map_store::residency::MAX_RESIDENT_ENVIRONMENT_PAGES`, private to its store.
const RESIDENT_PAGES: usize = 128;
pub(super) const WORKERS: usize = 4;
pub(super) const TILES: u64 = 2_048;
/// Two tiles per source sample and 16 by 16 pages per layer, like a prepared
/// map of 2,000 tiles.
pub(super) const AXIS: u16 = 1_024;
const PAGES_PER_AXIS: u16 = AXIS / 64;
/// Elevation, water, hydrology and modern land cover.
const PAGES: u64 = 4 * (PAGES_PER_AXIS as u64).pow(2);
pub(super) const FINE: u16 = MAX_MINIMAP_SAMPLES_PER_AXIS;
/// The coarse raster of `minimap_http`.
pub(super) const COARSE: u16 = 64;
/// Provider calls of one fine raster of `wide_pages`, as counted in review.
const FINE_CALLS: u64 = 5_300;

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    resident: usize,
    loads: u64,
    calls: u64,
}

pub(super) const UNLIMITED: Limits = Limits {
    resident: RESIDENT_PAGES,
    loads: u64::MAX,
    calls: u64::MAX,
};

/// Pages behind one lock and a least-recently-used cache, like the server's
/// residency. A source over its limit fails, so a run far above a bound stops
/// early instead of finishing.
#[derive(Debug)]
pub(super) struct PageSource {
    pages: HashMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
    /// Pages that fail with their own error.
    faults: HashMap<EnvironmentPageKey, EnvironmentPageError>,
    resident: Mutex<VecDeque<EnvironmentPageKey>>,
    limits: Limits,
    loads: AtomicU64,
    calls: AtomicU64,
    /// Raised by a test together with its cancellation.
    cancelled: AtomicBool,
    /// Calls that began after `cancelled` was raised.
    late_calls: AtomicU64,
}

impl PageSource {
    pub(super) fn new(pages: Vec<EnvironmentPage>, limits: Limits) -> Arc<Self> {
        Self::faulty(pages, limits, &[])
    }

    fn faulty(
        pages: Vec<EnvironmentPage>,
        limits: Limits,
        faults: &[(EnvironmentPageKey, EnvironmentPageError)],
    ) -> Arc<Self> {
        Arc::new(Self {
            pages: pages
                .into_iter()
                .map(|page| (page.key(), Arc::new(page)))
                .collect(),
            faults: faults.iter().copied().collect(),
            resident: Mutex::default(),
            limits,
            loads: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            late_calls: AtomicU64::new(0),
        })
    }

    pub(super) fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
}

impl EnvironmentPageProvider for PageSource {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if self.cancelled.load(Ordering::Relaxed) {
            self.late_calls.fetch_add(1, Ordering::Relaxed);
        }
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.calls.fetch_add(1, Ordering::Relaxed) >= self.limits.calls {
            return Err(EnvironmentPageError::Unavailable);
        }
        if let Some(error) = self.faults.get(&key) {
            return Err(*error);
        }
        let page = self
            .pages
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)?;
        let mut resident = self
            .resident
            .lock()
            .map_err(|_| EnvironmentPageError::Unavailable)?;
        if let Some(at) = resident.iter().rposition(|held| *held == key) {
            resident.remove(at);
        } else if self.loads.fetch_add(1, Ordering::Relaxed) >= self.limits.loads {
            return Err(EnvironmentPageError::Unavailable);
        }
        resident.push_back(key);
        if resident.len() > self.limits.resident {
            resident.pop_front();
        }
        Ok(page)
    }
}

/// Rolling dry land with an ocean in the west: no cliffs, no rivers, but the
/// hydrology index is present so every dry sample runs its river queries.
pub(super) fn wide_pages() -> Vec<EnvironmentPage> {
    let ripple = |at: i32, period: i32| period / 2 - (at.rem_euclid(period) - period / 2).abs();
    let mut pages = Vec::new();
    for y in 0..PAGES_PER_AXIS {
        for x in 0..PAGES_PER_AXIS {
            let (left, top) = (i32::from(x) * 64, i32::from(y) * 64);
            let cell = |index: i32| (left + index % 64, top + index / 64);
            pages.push(EnvironmentPage::Elevation(ElevationPage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                geographic_height_centimeters: (0..64 * 64)
                    .map(cell)
                    .map(|(x, y)| 20_000 + 300 * ripple(x, 96) + 250 * ripple(y, 80))
                    .collect(),
            }));
            pages.push(EnvironmentPage::Water(WaterPage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                ocean_coverage_percent: (0..64 * 64)
                    .map(cell)
                    .map(|(x, _)| if x < 128 { 100 } else { 0 })
                    .collect(),
                inland_coverage_percent: vec![0; 64 * 64],
            }));
            pages.push(EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                kind: vec![HydrologyKind::NoEvidence as u8; 64 * 64],
                method: vec![0; 64 * 64],
                river_segments: Vec::new(),
                water_model: None,
            }));
            pages.push(EnvironmentPage::ModernLandCover(ModernLandCoverPage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                worldcover_class: vec![0; 64 * 64],
            }));
        }
    }
    pages
}

pub(super) fn wide_environment() -> PreparedEnvironment {
    PreparedEnvironment {
        vegetation: None,
        hydrology_evidence: Some(HydrologyEvidenceIndex {
            samples_per_axis: AXIS,
            page_samples: 64,
            world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [3; 32],
            modern_land_cover_page_root: [4; 32],
            water_model: None,
        }),
        ..relief_environment(AXIS)
    }
}

pub(super) fn generator_over(
    source: Arc<dyn EnvironmentPageProvider>,
    tiles: u64,
    environment: PreparedEnvironment,
) -> MapChunkGenerator {
    MapChunkGenerator::new([41; 32], 1, tiles as i32)
        .with_page_provider(Ratio::new(30, 1).expect("ratio"), environment, source)
        .expect("page source")
}

pub(super) fn wide_raster(
    source: &Arc<PageSource>,
    samples: u16,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, MinimapError> {
    let generator = generator_over(source.clone(), TILES, wide_environment());
    minimap_raster_at_samples(&generator, TILES, 60_000, AXIS, samples, cancelled)
}

/// The raster's bytes and the page loads made, under a limit on those loads.
fn loads_of(samples: u16, limits: Limits) -> (Result<usize, MinimapError>, u64) {
    let source = PageSource::new(wide_pages(), limits);
    let raster = wide_raster(&source, samples, &|| false);
    let loads = source.loads.load(Ordering::Relaxed);
    (raster.map(|raster| raster.len()), loads)
}

/// One load for a page's own row of blocks and one for each neighbouring row
/// that reads its edge, with a third to spare. Rows visited across the whole
/// map reload every page once per row instead: about forty times each.
#[test]
fn a_fine_raster_loads_a_page_about_three_times_not_once_per_row() {
    let limits = Limits {
        loads: 4 * PAGES,
        ..UNLIMITED
    };
    let (raster, loads) = loads_of(FINE, limits);
    assert_eq!(
        raster,
        Ok(MINIMAP_HEADER_BYTES + usize::from(FINE).pow(2)),
        "stopped at {loads} loads of {PAGES} pages"
    );
    assert!(loads >= 3 * PAGES / 4, "the source is really read: {loads}");
}

/// Coarse samples are 32 tiles apart, so no neighbourhood reaches another
/// page and a block needs each of its pages once. The cache is one worker's
/// share of the server's, so a row of 64 pages cannot hide in it whatever the
/// machine's scheduling; a worker's own memo holds what its block needs.
#[test]
fn a_coarse_raster_loads_a_page_at_most_twice() {
    let limits = Limits {
        resident: RESIDENT_PAGES / WORKERS,
        loads: 2 * PAGES,
        ..UNLIMITED
    };
    let (raster, loads) = loads_of(COARSE, limits);
    assert_eq!(
        raster,
        Ok(MINIMAP_HEADER_BYTES + usize::from(COARSE).pow(2)),
        "stopped at {loads} loads of {PAGES} pages"
    );
    assert!(loads >= 3 * PAGES / 4, "the source is really read: {loads}");
}

/// Without a memo every lookup takes the provider's lock, hundreds of times
/// per sample. The limit, 21,200, is four times the calls counted in review.
/// Scheduling only decides which blocks a worker met before, so a block starts
/// with other pages in its memo: about 32 calls for each of 64 blocks, 2,048
/// in all, which leaves the total far under the limit.
#[test]
fn a_fine_raster_asks_the_shared_provider_about_twenty_times_per_thousand_samples() {
    let limits = Limits {
        calls: 4 * FINE_CALLS,
        ..UNLIMITED
    };
    let source = PageSource::new(wide_pages(), limits);
    let raster = wide_raster(&source, FINE, &|| false);
    assert!(
        raster.is_ok(),
        "over {} provider calls: {:?}",
        limits.calls,
        raster.err()
    );
}

/// Three pages hold the whole source, so a worker's own memo answers every
/// repeat of every neighbourhood: one call per sixteen samples leaves room for
/// a fresh memo per block.
#[test]
fn each_worker_answers_repeated_pages_from_its_own_memo() {
    let (tiles, axis, samples) = (512, 16, 64_u16);
    let limits = Limits {
        calls: u64::from(samples).pow(2) / 16,
        ..UNLIMITED
    };
    let source = PageSource::new(relief_pages(axis, 20, |_, _| 30_000), limits);
    let generator = generator_over(source, tiles, relief_environment(axis));
    let raster = minimap_raster_at_samples(&generator, tiles, 60_000, axis, samples, &|| false);
    assert!(
        raster.is_ok(),
        "over {} provider calls for three pages: {:?}",
        limits.calls,
        raster.err()
    );
}

/// Every page lookup and memo read polls the cancellation, so polls count the
/// landscape work, memo hits included: provider calls cannot show the rock
/// search once a memo answers it. The raster reads no passability, so loose
/// rocks must not change the work.
#[test]
fn a_raster_does_no_rock_work() {
    let polls_of = |generator: &MapChunkGenerator| {
        let polls = AtomicU64::new(0);
        minimap_raster_at_samples(generator, 512, 60_000, 16, 128, &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        })
        .expect("raster");
        polls.into_inner()
    };
    let rocky = relief_source(512, 16, 20, |_, _| 30_000);
    let bare = rocky.clone().without_loose_rocks();
    let (rocky, bare) = (polls_of(&rocky), polls_of(&bare));
    // A quarter of slack for which worker meets which block first.
    assert!(
        rocky * 4 <= bare * 5,
        "{rocky} polls with loose rocks against {bare} without"
    );
}

#[test]
fn cancellation_inside_a_block_stops_promptly_and_returns_no_raster() {
    let complete = AtomicU64::new(0);
    wide_raster(&PageSource::new(wide_pages(), UNLIMITED), COARSE, &|| {
        complete.fetch_add(1, Ordering::Relaxed);
        false
    })
    .expect("complete raster");
    let complete = complete.into_inner();
    for raised_at in [0, complete / 3, complete * 2 / 3] {
        let source = PageSource::new(wide_pages(), UNLIMITED);
        let polls = AtomicU64::new(0);
        let raster = wide_raster(&source, COARSE, &|| {
            let raised = polls.fetch_add(1, Ordering::Relaxed) >= raised_at;
            if raised {
                source.cancelled.store(true, Ordering::Relaxed);
            }
            raised
        });
        assert_eq!(
            raster,
            Err(MinimapError::Page(EnvironmentPageError::Cancelled)),
            "raised at poll {raised_at} of {complete}"
        );
        // A worker already past its own poll may finish one provider call.
        let late = source.late_calls.load(Ordering::Relaxed);
        assert!(late <= 2 * WORKERS as u64, "{late} late provider calls");
        let after = polls.into_inner() - raised_at;
        assert!(after <= 2_048, "{after} polls after the cancellation");
    }
}

#[test]
fn a_page_missing_anywhere_fails_the_whole_raster() {
    let last = PAGES_PER_AXIS - 1;
    for (layer, x, y) in [
        (PageLayer::Elevation, 0, 0),
        (PageLayer::Water, 8, 8),
        (PageLayer::HydrologyEvidence, last, last),
    ] {
        let key = EnvironmentPageKey {
            layer,
            level: 0,
            x,
            y,
        };
        let mut pages = wide_pages();
        pages.retain(|page| page.key() != key);
        assert_eq!(
            wide_raster(&PageSource::new(pages, UNLIMITED), COARSE, &|| false),
            Err(MinimapError::Page(EnvironmentPageError::Missing)),
            "{key:?}"
        );
    }
}

/// Block 0 reads its failing page near its thirtieth row of samples and block
/// 1 reads its own in its first row, beyond block 0's neighbourhood: the later
/// block fails first and must not decide, whatever the two errors are.
#[test]
fn the_lowest_failing_block_decides_the_error_on_every_run() {
    use EnvironmentPageError::{Corrupt, Missing};
    let elevation = |x, y| EnvironmentPageKey {
        layer: PageLayer::Elevation,
        level: 0,
        x,
        y,
    };
    for (lowest, other) in [(Missing, Corrupt), (Corrupt, Missing)] {
        for run in 0..3 {
            let faults = [(elevation(0, 1), lowest), (elevation(3, 0), other)];
            let source = PageSource::faulty(wide_pages(), UNLIMITED, &faults);
            assert_eq!(
                wide_raster(&source, FINE, &|| false),
                Err(MinimapError::Page(lowest)),
                "run {run}"
            );
        }
    }
}
