//! Public and legacy detailed-preparation entry points.
use super::{DemResolution, prepare_with_staging_and_corrections};
use crate::GeodataError;
use aoe_map::{MapPackage, MapRequest};
use std::path::PathBuf;

pub fn prepare_detailed_directory(
    cache_root: PathBuf,
    output_directory: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    resolution: DemResolution,
) -> Result<MapPackage, GeodataError> {
    prepare_with_staging(
        cache_root,
        output_directory,
        request,
        samples_per_axis,
        resolution,
        None,
    )
}

fn prepare_with_staging(
    cache_root: PathBuf,
    output_directory: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    resolution: DemResolution,
    staging_root: Option<PathBuf>,
) -> Result<MapPackage, GeodataError> {
    prepare_with_staging_and_corrections(
        cache_root,
        output_directory,
        request,
        samples_per_axis,
        resolution,
        staging_root,
        None,
    )
}
