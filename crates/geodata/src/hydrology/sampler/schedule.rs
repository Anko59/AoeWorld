//! Page sampling across worker threads, merged in page order by the caller.
use super::super::hydrology_sampling::local_transform;
use super::super::{GeodataError, HydrologyPage, MapRequest, ModernLandCoverPage, Tile};
use super::Sampler;
use super::topology::{RiverCellProjection, RiverReachMetadata};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct Reopen {
    pub(super) tiles: Vec<Tile>,
    pub(super) lakes: PathBuf,
    pub(super) rivers: Option<PathBuf>,
}

/// Everything a page contributes; merged in page order by the owning sampler.
pub(super) struct PageOutput {
    pub(super) hydrology: HydrologyPage,
    pub(super) land_cover: ModernLandCoverPage,
    pub(super) reaches: Vec<(u32, RiverReachMetadata)>,
    pub(super) cells: Vec<(usize, RiverCellProjection)>,
}

/// Coordinate transforms are built once per thread, not per page.
pub(super) struct Projections {
    pub(super) local_to_wgs84: CoordTransform,
    pub(super) wgs84_to_local: CoordTransform,
}

impl Projections {
    pub(super) fn new(request: MapRequest) -> Result<Self, GeodataError> {
        let definition =
            crate::local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
        let mut target =
            SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
        let mut wgs84 = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
        target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        wgs84.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        Ok(Self {
            local_to_wgs84: CoordTransform::new(&target, &wgs84)
                .map_err(|_| GeodataError::Projection)?,
            wgs84_to_local: local_transform(&definition)?,
        })
    }
}

/// Worker threads used for page sampling: the cores, capped because the
/// machine is shared and each thread holds its own source handles.
fn worker_count(pages: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(6)
        .min(pages)
        .max(1)
}

impl Sampler {
    /// A page-sampling-only copy with its own dataset handles.
    fn worker(
        request: MapRequest,
        axis: u16,
        ocean: &[u8],
        reopen: &Reopen,
    ) -> Result<Self, GeodataError> {
        let mut worker = Self::open(
            request,
            axis,
            reopen.tiles.clone(),
            ocean.to_vec(),
            &reopen.lakes,
            reopen.rivers.as_deref(),
        )?;
        worker.river_cells = Vec::new();
        Ok(worker)
    }

    pub(super) fn sample_pages(
        &mut self,
        coordinates: &[(u16, u16)],
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<Result<PageOutput, GeodataError>>, GeodataError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let threads = if self.reopen.is_some() {
            worker_count(coordinates.len())
        } else {
            1
        };
        let next = AtomicUsize::new(0);
        let slots = std::sync::Mutex::new(
            (0..coordinates.len())
                .map(|_| None)
                .collect::<Vec<Option<Result<PageOutput, GeodataError>>>>(),
        );
        let run = |sampler: &mut Self| {
            let projections = match Projections::new(sampler.request) {
                Ok(projections) => projections,
                Err(error) => return Some(error),
            };
            loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let &(x, y) = coordinates.get(index)?;
                if cancelled.load(Ordering::SeqCst) {
                    return Some(crate::CacheError::Cancelled.into());
                }
                let output = sampler.page(x, y, &projections);
                let failed = output.is_err();
                slots
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)[index] = Some(output);
                if failed {
                    // Later pages cannot change the first error in page order.
                    next.store(coordinates.len(), Ordering::SeqCst);
                    return None;
                }
            }
        };
        let failure = if threads <= 1 {
            run(self)
        } else {
            let reopen = self.reopen.clone().ok_or(GeodataError::Preparation(
                "sampler cannot reopen its sources",
            ))?;
            let (request, axis, ocean) = (self.request, self.axis, self.ocean.clone());
            let failures = std::thread::scope(|scope| {
                let handles = (1..threads)
                    .map(|_| {
                        scope.spawn(|| match Self::worker(request, axis, &ocean, &reopen) {
                            Ok(mut worker) => run(&mut worker),
                            Err(error) => Some(error),
                        })
                    })
                    .collect::<Vec<_>>();
                // This thread keeps working through its own handles.
                let mut failures = vec![run(self)];
                failures.extend(handles.into_iter().map(|handle| {
                    handle.join().unwrap_or(Some(GeodataError::Preparation(
                        "page sampling thread panicked",
                    )))
                }));
                failures
            });
            failures.into_iter().flatten().next()
        };
        let mut slots = slots
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A failure to start (not tied to a page) or cancellation wins when no
        // page reported one; a page error stays in its own slot.
        if let Some(error) = failure
            && let Some(first) = slots.iter_mut().find(|slot| slot.is_none())
        {
            *first = Some(Err(error));
        }
        Ok(slots.into_iter().flatten().collect())
    }
}
