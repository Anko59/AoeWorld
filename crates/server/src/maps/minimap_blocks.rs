//! Block traversal of one raster: which samples a worker visits together and
//! which threads run it.
//!
//! A sample reads its neighbourhood from source pages of 64 samples per axis
//! and a worker remembers its last 32 pages. A row across the whole map touches
//! more pages than that, so the next row loads all of them again; a block
//! touches few pages and is then done with them.
//!
//! With 1,024 source samples per axis a page is 32 samples of the 512-sample
//! raster, with 512 it is 64: a block of 64 starts on a page edge in both. It is
//! also the widest whose pages fit the memo. At a page edge a row of samples
//! reads two page rows, each of two pages and their two neighbours: 24 pages
//! of elevation, water and hydrology, with two of every other layer.
//!
//! A coarser raster takes a quarter of its axis: two or four whole pages. A
//! block of 64 would be the whole coarse raster, in rows across the map again.
use super::{
    MINIMAP_MIN_RIVER_WIDTH_UNITS, MINIMAP_WARP_MARGIN_TILES, MinimapClass, MinimapError, classify,
    minimap_tile,
};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, MapChunkGenerator, Provenance, RasterSurface, WaterKind};
use std::{
    ops::Range,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Threads that all fine rasters share.
const FINE_WORKERS: usize = 4;
/// Axis of the coarse endpoint's raster. The finest raster of a map of no more
/// tiles is as small and runs as coarse too.
const COARSE_SAMPLES: u16 = 64;
const MIN_BLOCK_SAMPLES: usize = 16;
const MAX_BLOCK_SAMPLES: usize = 64;

static FINE_POOL: LazyLock<Option<rayon::ThreadPool>> = LazyLock::new(|| pool(FINE_WORKERS));
static COARSE_POOL: LazyLock<Option<rayon::ThreadPool>> = LazyLock::new(|| pool(1));

fn pool(threads: usize) -> Option<rayon::ThreadPool> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .ok()
}

/// A coarse raster has its own thread, so fine rasters that hold all of theirs
/// do not delay it. A pool that could not be built stays an error.
fn pool_for(samples: u16) -> Result<&'static rayon::ThreadPool, MinimapError> {
    let pool = if samples <= COARSE_SAMPLES {
        &COARSE_POOL
    } else {
        &FINE_POOL
    };
    LazyLock::force(pool)
        .as_ref()
        .ok_or(MinimapError::Page(EnvironmentPageError::Unavailable))
}

/// Class and geographic height in centimetres of one sample.
pub(super) type Sample = (MinimapClass, i32);

/// The square blocks of one raster, numbered in row-major order. The last
/// block of an axis is cut at the raster's edge.
#[derive(Clone, Copy)]
struct Blocks {
    samples: usize,
    side: usize,
    per_axis: usize,
}

impl Blocks {
    fn new(samples: usize) -> Self {
        let side = (samples / 4)
            .next_power_of_two()
            .clamp(MIN_BLOCK_SAMPLES, MAX_BLOCK_SAMPLES);
        Self {
            samples,
            side,
            per_axis: samples.div_ceil(side),
        }
    }

    fn count(self) -> usize {
        self.per_axis.pow(2)
    }

    /// Sample columns and rows of one block.
    fn bounds(self, block: usize) -> (Range<usize>, Range<usize>) {
        let span = |at: usize| {
            let start = at * self.side;
            start..(start + self.side).min(self.samples)
        };
        (span(block % self.per_axis), span(block / self.per_axis))
    }
}

/// Finished blocks in row-major sample order, and the first block that failed.
struct Progress {
    samples: Vec<Sample>,
    failure: Option<(usize, MinimapError)>,
}

impl Progress {
    fn write(&mut self, blocks: Blocks, block: usize, samples: &[Sample]) {
        let (columns, rows) = blocks.bounds(block);
        for (row, samples) in rows.zip(samples.chunks_exact(columns.len())) {
            let start = row * blocks.samples + columns.start;
            self.samples[start..start + columns.len()].copy_from_slice(samples);
        }
    }

    fn fail(&mut self, block: usize, error: MinimapError) {
        if self.failure.is_none_or(|(first, _)| block < first) {
            self.failure = Some((block, error));
        }
    }
}

/// Starts no new block once a worker leaves, by an error or a panic as well;
/// without a block left this changes nothing. Blocks that have started end,
/// then the scope raises a panic again in the caller.
struct StopOnLeave<'a> {
    next: &'a AtomicUsize,
    end: usize,
}

impl Drop for StopOnLeave<'_> {
    fn drop(&mut self) {
        self.next.store(self.end, Ordering::Relaxed);
    }
}

/// The point queries of one raster.
pub(super) struct Sampler<'a> {
    pub(super) generator: &'a MapChunkGenerator,
    pub(super) tiles_per_side: u64,
    pub(super) samples: u16,
    pub(super) cancelled: &'a (dyn Fn() -> bool + Sync),
}

impl Sampler<'_> {
    /// Every sample in row-major order, or an error and no sample at all.
    /// Blocks start in order and a started block ends, so every block below a
    /// failing one is tried: the error is that of the lowest failing block,
    /// whatever the scheduling. Only a cancellation depends on its moment.
    pub(super) fn run(&self) -> Result<Vec<Sample>, MinimapError> {
        let pool = pool_for(self.samples)?;
        let side = usize::from(self.samples);
        let blocks = Blocks::new(side);
        let next = AtomicUsize::new(0);
        let progress = Mutex::new(Progress {
            samples: vec![(MinimapClass::Grass, 0); side * side],
            failure: None,
        });
        pool.in_place_scope(|scope| {
            for _ in 0..pool.current_num_threads() {
                scope.spawn(|_| self.work(blocks, &next, &progress));
            }
        });
        let progress = progress
            .into_inner()
            .map_err(|_| MinimapError::Page(EnvironmentPageError::Unavailable))?;
        match progress.failure {
            Some((_, error)) => Err(error),
            None => Ok(progress.samples),
        }
    }

    /// One worker with its own memos: takes blocks until none is left or a
    /// worker left early. It runs nothing else in between, so a later raster
    /// waits in the pool's queue and cannot pause one that has started.
    fn work(&self, blocks: Blocks, next: &AtomicUsize, progress: &Mutex<Progress>) {
        let _stop = StopOnLeave {
            next,
            end: blocks.count(),
        };
        let generator = self.generator.navigation_preparation_worker();
        loop {
            let block = next.fetch_add(1, Ordering::Relaxed);
            if block >= blocks.count() {
                return;
            }
            let result = self.block(&generator, blocks.bounds(block));
            // Queries run without the lock, so their panic does not poison it.
            let Ok(mut progress) = progress.lock() else {
                return;
            };
            match result {
                Ok(samples) => progress.write(blocks, block, &samples),
                Err(error) => {
                    progress.fail(block, error);
                    return;
                }
            }
        }
    }

    /// The samples of one block, row by row. Every point query polls the
    /// cancellation before its first read and again at each page or memo read.
    fn block(
        &self,
        generator: &MapChunkGenerator,
        (columns, rows): (Range<usize>, Range<usize>),
    ) -> Result<Vec<Sample>, MinimapError> {
        let mut samples = Vec::with_capacity(columns.len() * rows.len());
        for y in rows {
            for x in columns.clone() {
                samples.push(self.sample(generator, x, y)?);
            }
        }
        Ok(samples)
    }

    fn sample(
        &self,
        generator: &MapChunkGenerator,
        x: usize,
        y: usize,
    ) -> Result<Sample, MinimapError> {
        let tile = TileCoord::new(self.tile(x)?, self.tile(y)?);
        let surface = self
            .surface(generator, tile)?
            .ok_or(MinimapError::OutsideMap)?;
        let class = self.drawn_class(generator, tile, surface)?;
        Ok((class, surface.geographic_height_centimeters))
    }

    fn tile(&self, index: usize) -> Result<i32, MinimapError> {
        minimap_tile(self.tiles_per_side, self.samples, index as u16)
            .ok_or(MinimapError::OutsideMap)
    }

    fn surface(
        &self,
        generator: &MapChunkGenerator,
        tile: TileCoord,
    ) -> Result<Option<RasterSurface>, MinimapError> {
        generator
            .raster_surface_with_cancel(tile, self.cancelled)
            .map_err(MinimapError::Page)
    }

    /// The class to draw: rivers count by the width of the nearby centreline.
    /// Rivers without source geometry keep their tile width.
    fn drawn_class(
        &self,
        generator: &MapChunkGenerator,
        tile: TileCoord,
        surface: RasterSurface,
    ) -> Result<MinimapClass, MinimapError> {
        let class = classify(surface);
        if !generator.has_river_geometry()
            || matches!(class, MinimapClass::Ocean | MinimapClass::Lake)
        {
            return Ok(class);
        }
        let spacing = self.tiles_per_side.div_ceil(u64::from(self.samples));
        let footprint = (spacing * 181).div_ceil(256) as i32;
        if class == MinimapClass::River {
            // Physical water counts only where a drawn river is nearby.
            let reach = footprint + MINIMAP_WARP_MARGIN_TILES;
            if self.drawn_river_within(generator, tile, reach)? {
                return Ok(class);
            }
            return self.surrounding_land_class(generator, tile, reach + 2);
        }
        // A cited land correction is authoritative at this raster sample.
        // Ordinary dry sample centres still discover subpixel rivers.
        let corrected_land = surface.water == WaterKind::None
            && surface.water_provenance == Provenance::HistoricallyCorrected;
        if !corrected_land && self.drawn_river_within(generator, tile, footprint)? {
            return Ok(MinimapClass::River);
        }
        Ok(class)
    }

    fn drawn_river_within(
        &self,
        generator: &MapChunkGenerator,
        tile: TileCoord,
        radius: i32,
    ) -> Result<bool, MinimapError> {
        generator
            .river_within_with_cancel(tile, radius, MINIMAP_MIN_RIVER_WIDTH_UNITS, self.cancelled)
            .map_err(MinimapError::Page)
    }

    /// Class of the dry land around an omitted stream: the first dry sample on
    /// a ring beyond the stream's bank, so a desert or a snowfield keeps its
    /// own colour. Grass only where no dry land is within reach.
    fn surrounding_land_class(
        &self,
        generator: &MapChunkGenerator,
        tile: TileCoord,
        reach: i32,
    ) -> Result<MinimapClass, MinimapError> {
        for distance in [reach, reach * 2] {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let probe = TileCoord::new(
                    tile.x.saturating_add(dx * distance),
                    tile.y.saturating_add(dy * distance),
                );
                let Some(surface) = self.surface(generator, probe)? else {
                    continue;
                };
                if surface.water == WaterKind::None {
                    return Ok(classify(surface));
                }
            }
        }
        Ok(MinimapClass::Grass)
    }
}
