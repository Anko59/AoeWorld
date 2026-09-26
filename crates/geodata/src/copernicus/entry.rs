use super::{DemResolution, GeodataError, MapPackage, MapRequest, PathBuf, prepare_with_staging};

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
    water_corrections: Option<aoe_map::WaterCorrectionDocument>,
) -> Result<MapPackage, GeodataError> {
    prepare_with_staging(
        cache_root,
        output_directory,
        request,
        samples_per_axis,
        resolution,
        None,
        water_corrections,
    )
}
