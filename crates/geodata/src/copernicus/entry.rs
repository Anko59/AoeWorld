//! Detailed-preparation entry points and correction inputs.
use super::{DemResolution, prepare_with_staging_and_corrections};
use crate::GeodataError;
use aoe_map::{MapPackage, MapRequest, WaterCorrectionDocument};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct DetailedCorrections<'a> {
    pub historical: Option<&'a crate::GeographicHistoricalCorrectionDocument>,
    pub water: Option<WaterCorrectionDocument>,
    pub vegetation: Option<&'a crate::VegetationPatchDocument>,
}

pub fn prepare_detailed_directory(
    cache_root: PathBuf,
    output_directory: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    resolution: DemResolution,
) -> Result<MapPackage, GeodataError> {
    prepare_detailed_directory_with_water_corrections(
        cache_root,
        output_directory,
        request,
        samples_per_axis,
        resolution,
        None,
    )
}

pub fn prepare_detailed_directory_with_water_corrections(
    cache_root: PathBuf,
    output_directory: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    resolution: DemResolution,
    water_corrections: Option<WaterCorrectionDocument>,
) -> Result<MapPackage, GeodataError> {
    prepare_with_staging_and_corrections(
        cache_root,
        output_directory,
        request,
        samples_per_axis,
        resolution,
        None,
        DetailedCorrections {
            historical: None,
            water: water_corrections,
            vegetation: None,
        },
    )
}

pub(super) fn tool_version() -> String {
    let gdal = gdal::version::VersionInfo::release_name();
    let proj = gdal::version::VersionInfo::build_info()
        .get("PROJ_RUNTIME_VERSION")
        .cloned()
        .unwrap_or_else(|| "unknown".to_owned());
    format!("GDAL {gdal} / PROJ {proj}")
}

/// Sample the acquired DEM at the bounded water grid. Holding only these
/// level-zero pages costs at most 1024² i32 heights (4 MiB), independent of
/// the larger detailed terrain pyramid which is streamed separately.
pub(super) fn apply_detailed_water_model(
    sampler: &mut super::Sampler,
    hydrology: &mut crate::PreparedHydrology,
    request: MapRequest,
    corrections: WaterCorrectionDocument,
) -> Result<(), GeodataError> {
    let axis = hydrology.samples_per_axis;
    if !(2..=crate::MAX_HYDROLOGY_SAMPLES_PER_AXIS).contains(&axis) {
        return Err(GeodataError::Preparation(
            "invalid detailed water elevation axis",
        ));
    }
    let page_axis = axis.div_ceil(super::PAGE);
    let mut elevations = Vec::with_capacity(usize::from(page_axis).pow(2));
    let total = u64::from(page_axis).pow(2);
    for y in 0..page_axis {
        for x in 0..page_axis {
            elevations.push(sampler.page(axis, 0, x, y)?);
            crate::preparation_progress::count(
                crate::preparation_progress::Phase::SamplingWater,
                Some("Sampling water elevation"),
                elevations.len() as u64,
                total,
                crate::preparation_progress::Unit::Pages,
            );
        }
    }
    crate::hydrology::apply_water_model(hydrology, request, &elevations, corrections)
}
