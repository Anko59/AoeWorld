use super::*;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};

pub(super) const MODERN_NOT_REQUESTED: &str =
    "modern-landcover=not-requested;modern-class=0-nodata;2021=classification-legend-only";

impl PreparedHydrology {
    /// Validates the bounded vector request and correction binding without acquisition.
    pub fn validate_vectors_request(
        request: MapRequest,
        axis: u16,
        corrections: &WaterCorrectionDocument,
        cancelled: &AtomicBool,
    ) -> Result<(), GeodataError> {
        check_cancelled(cancelled)?;
        let request = validate_vector_request(request, axis)?;
        corrections.validate_for(request, axis).map_err(|_| {
            GeodataError::Preparation("water corrections do not match request grid")
        })?;
        validate_vector_footprint(request, axis, cancelled)
    }

    /// Vector evidence using already prepared overview context. No overview,
    /// WorldCover catalog, HEAD, ZIP, or raster is acquired by this entry point.
    /// The caller supplies its current policy's correction document. Vector extents
    /// remain modern mapped evidence; model target year is not an observation year.
    pub fn prepare_vectors(
        cache_root: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
        overview_elevation_pages: &[aoe_map::ElevationPage],
        overview_water_pages: &[aoe_map::WaterPage],
        corrections: WaterCorrectionDocument,
        cancelled: &AtomicBool,
    ) -> Result<Self, GeodataError> {
        super::prepare_hydrology_vectors(
            cache_root,
            request,
            samples_per_axis,
            overview_elevation_pages,
            overview_water_pages,
            corrections,
            cancelled,
        )
    }

    /// In vector-only output the wire-required 2021 field identifies the class
    /// legend, not observed coverage. Modern pages contain only unobserved nodata.
    /// This marker also occurs in every actually acquired vector source lock.
    /// WorldCover-backed output carries no such marker.
    pub fn modern_land_cover_preprocessing(&self) -> Option<&'static str> {
        self.source_locks
            .iter()
            .any(|lock| lock.preprocessing_version.contains(MODERN_NOT_REQUESTED))
            .then_some(MODERN_NOT_REQUESTED)
    }
}

/// Prepares pinned HydroLAKES and the footprint's HydroRIVERS region without requesting modern
/// land cover. Input overview pages are context only, not newly acquired locks:
/// callers must retain their overview source locks when packaging both results.
/// The returned locks describe only the two vector datasets actually acquired.
/// Modern page roots remain wire-compatible, but all modern classes are nodata.
pub fn prepare_hydrology_vectors(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    overview_elevation_pages: &[aoe_map::ElevationPage],
    overview_water_pages: &[aoe_map::WaterPage],
    corrections: WaterCorrectionDocument,
    cancelled: &AtomicBool,
) -> Result<PreparedHydrology, GeodataError> {
    check_cancelled(cancelled)?;
    let request = validate_vector_request(request, samples_per_axis)?;
    corrections
        .validate_for(request, samples_per_axis)
        .map_err(|_| GeodataError::Preparation("water corrections do not match request grid"))?;
    let footprint = project_river_footprint(request, samples_per_axis, cancelled)?;
    footprint.check_binding(request, samples_per_axis)?;
    prepare_vectors_validated(
        cache_root,
        footprint,
        overview_elevation_pages,
        overview_water_pages,
        corrections,
        cancelled,
    )
}

pub(crate) fn prepare_vectors_validated(
    cache_root: PathBuf,
    footprint: RiverFootprint,
    overview_elevation_pages: &[aoe_map::ElevationPage],
    overview_water_pages: &[aoe_map::WaterPage],
    corrections: WaterCorrectionDocument,
    cancelled: &AtomicBool,
) -> Result<PreparedHydrology, GeodataError> {
    let request = footprint.request;
    let samples_per_axis = footprint.axis;
    footprint.check_binding(request, samples_per_axis)?;
    check_cancelled(cancelled)?;
    corrections
        .validate_for(request, samples_per_axis)
        .map_err(|_| GeodataError::Preparation("water corrections do not match request grid"))?;
    // Reject malformed supplied context before any cache or provider access.
    let elevation_context = super::overview_elevation_context(overview_elevation_pages)?;
    let ocean_axis = hydrology_sampling::ocean_source_axis(overview_water_pages)?;
    hydrology_sampling::resample_ocean_coverage(
        ocean_axis,
        samples_per_axis,
        overview_water_pages,
    )?;
    let plan = HydrologySourcePlan {
        worldcover: Vec::new(),
        sources: hydrology_vector_sources(footprint.rivers()?),
        footprint,
    };
    check_cancelled(cancelled)?;
    let mut prepared = prepare_with_plan(
        cache_root,
        request,
        samples_per_axis,
        plan,
        overview_water_pages,
        HydrologySelection::Vectors(cancelled),
    )?;
    check_cancelled(cancelled)?;
    crate::preparation_progress::trace_mark("hydrology evidence done");
    apply_water_model(&mut prepared, request, &elevation_context, corrections)?;
    check_cancelled(cancelled)?;
    Ok(prepared)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), GeodataError> {
    if cancelled.load(Ordering::SeqCst) {
        Err(crate::CacheError::Cancelled.into())
    } else {
        Ok(())
    }
}

fn validate_vector_request(request: MapRequest, axis: u16) -> Result<MapRequest, GeodataError> {
    if !(2..=MAX_HYDROLOGY_SAMPLES_PER_AXIS).contains(&axis) {
        return Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis",
        ));
    }
    request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))
}

// Check every cell's vertices, edge midpoints and center, not a degree approximation
// or only four corners. Row-sized coordinate buffers retain the existing axis cap.
// Coverage uses each SHA-pinned regional shapefile header extent, inset by query
// padding; the first wired region containing the whole footprint supplies rivers.
#[derive(Clone, Debug)]
pub(crate) struct RiverFootprint {
    request: MapRequest,
    axis: u16,
    rivers: Option<&'static HydroRiversRegion>,
}

impl RiverFootprint {
    pub(crate) fn covered(&self) -> bool {
        self.rivers.is_some()
    }
    /// The HydroRIVERS region selected for this footprint.
    pub(crate) fn rivers(&self) -> Result<&'static HydroRiversRegion, GeodataError> {
        self.rivers.ok_or(GeodataError::Preparation(
            "vector hydrology footprint is outside the pinned river dataset extent",
        ))
    }
    pub(crate) fn check_binding(&self, request: MapRequest, axis: u16) -> Result<(), GeodataError> {
        if request.normalized().ok() != Some(self.request) || axis != self.axis {
            return Err(GeodataError::Preparation(
                "hydrology preflight grid does not match sampling request",
            ));
        }
        self.rivers().map(|_| ())
    }
}

pub(super) fn validate_vector_footprint(
    request: MapRequest,
    axis: u16,
    cancelled: &AtomicBool,
) -> Result<(), GeodataError> {
    project_river_footprint(request, axis, cancelled)?.check_binding(request, axis)
}

pub(crate) fn project_river_footprint(
    request: MapRequest,
    axis: u16,
    cancelled: &AtomicBool,
) -> Result<RiverFootprint, GeodataError> {
    check_cancelled(cancelled)?;
    let request = validate_vector_request(request, axis)?;
    let side = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?
        .effective_side_meters as f64;
    let definition =
        crate::local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let mut local =
        SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
    let mut geographic = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    local.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    geographic.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform =
        CoordTransform::new(&local, &geographic).map_err(|_| GeodataError::Projection)?;
    let spacing = side / f64::from(axis);
    let padding = 0.01;
    let mut candidates = vec![true; HYDRORIVERS_REGIONS.len()];
    for row in 0..=u32::from(axis) * 2 {
        check_cancelled(cancelled)?;
        let mut longitude = (0..=u32::from(axis) * 2)
            .map(|column| -side / 2.0 + f64::from(column) * spacing / 2.0)
            .collect::<Vec<_>>();
        let mut latitude = vec![side / 2.0 - f64::from(row) * spacing / 2.0; longitude.len()];
        transform
            .transform_coords(&mut longitude, &mut latitude, &mut [])
            .map_err(|_| GeodataError::Coordinate)?;
        for (candidate, region) in candidates.iter_mut().zip(&HYDRORIVERS_REGIONS) {
            let extent = region.extent;
            let longitudes = extent.min_longitude + padding..=extent.max_longitude - padding;
            let latitudes = extent.min_latitude + padding..=extent.max_latitude - padding;
            *candidate = *candidate
                && longitude.iter().zip(&latitude).all(|(&lon, &lat)| {
                    lon.is_finite()
                        && lat.is_finite()
                        && longitudes.contains(&lon)
                        && latitudes.contains(&lat)
                });
        }
        if !candidates.contains(&true) {
            break;
        }
    }
    let rivers = candidates
        .iter()
        .position(|candidate| *candidate)
        .map(|index| &HYDRORIVERS_REGIONS[index]);
    Ok(RiverFootprint {
        request,
        axis,
        rivers,
    })
}

#[cfg(test)]
#[path = "tests/vectors.rs"]
mod tests;
