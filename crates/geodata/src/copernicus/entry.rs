//! Detailed-preparation entry points and correction inputs.
use super::{
    Bounds, DemResolution, TileCoverage, prepare_with_staging_and_corrections,
    source_backed_overview_ocean,
};
use crate::GeodataError;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentalProvenance, LayerProvenance, MAP_SCHEMA_VERSION,
    MapPackage, MapRequest, PreparedEnvironment, ProjectionMetadata, VerticalDatum,
    WaterCorrectionDocument,
};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct DetailedCorrections<'a> {
    pub historical: Option<&'a crate::GeographicHistoricalCorrectionDocument>,
    pub water: Option<WaterCorrectionDocument>,
    pub vegetation: Option<&'a crate::VegetationPatchDocument>,
}

pub(super) struct AcquiredDetailedInputs {
    pub(super) bounds: Bounds,
    pub(super) overview: crate::PreparedOverview,
    pub(super) hydrology: crate::PreparedHydrology,
    pub(super) coverage: TileCoverage,
}

/// Completes detailed package assembly from inputs whose acquisition and
/// source-policy checks have already succeeded.
pub(super) fn assemble_acquired_package(
    request: MapRequest,
    effective_side_meters: u64,
    samples_per_axis: u16,
    output_directory: &Path,
    staging_root: &Path,
    water_corrections: WaterCorrectionDocument,
    inputs: AcquiredDetailedInputs,
) -> Result<MapPackage, GeodataError> {
    let AcquiredDetailedInputs {
        bounds,
        overview,
        mut hydrology,
        coverage,
    } = inputs;
    let stage = super::Stage::new(staging_root)?;
    let overview_ocean = source_backed_overview_ocean(&overview)?;
    let mut sampler = super::sampler::Sampler::new(
        request,
        effective_side_meters,
        bounds,
        coverage.absent_tiles,
        coverage.tiles,
        overview_ocean,
    )?;
    apply_detailed_water_model(&mut sampler, &mut hydrology, request, water_corrections)?;
    super::pyramid::store_hydrology_evidence(&stage, &hydrology)?;
    let fields = super::pyramid::build_pyramids(
        &mut sampler,
        &stage,
        samples_per_axis,
        &overview,
        &hydrology,
    )?;
    let mut sources = vec![
        overview.source_lock,
        overview.water_source_lock,
        overview.vegetation_source_lock,
        overview.vegetation_classes_source_lock,
        overview.hyde_baseline_source_lock,
        overview.hyde_supplementary_source_lock,
        overview.hyde_readme_source_lock,
    ];
    sources.extend(
        sampler
            .tiles
            .iter()
            .map(|tile| {
                tile.lock.to_map_source_lock(
                    crate::acquisition_marker(),
                    "copernicus-cog-page-v1".to_owned(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    sources.extend(hydrology.source_locks.iter().cloned());
    let environment = PreparedEnvironment {
        samples_per_axis,
        geographic_millimeters_per_sample: effective_side_meters
            .checked_mul(1_000)
            .ok_or(GeodataError::Preparation("sample spacing overflows"))?
            .div_ceil(u64::from(samples_per_axis)),
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: fields.elevation,
        water: Some(fields.water),
        vegetation: Some(fields.vegetation),
        historical_land_use: Some(fields.historical_land_use),
        hydrology_evidence: Some(hydrology.evidence_index.clone()),
    };
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        request,
        sources,
        ProjectionMetadata {
            horizontal_crs: crate::local_aeqd_definition(
                request.center_latitude_e7,
                request.center_longitude_e7,
            ),
            vertical_datum: VerticalDatum::Egm2008Orthometric,
            tool_version: tool_version(),
        },
        EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: overview.provenance.water,
            vegetation: overview.provenance.vegetation,
            historical_land_use: overview.provenance.historical_land_use,
        },
        environment,
    )?;
    package.validate()?;
    super::stage::publish_staged_pages(&stage, output_directory, &package, samples_per_axis)?;
    crate::directory::publish_streaming_manifest(output_directory, &package)?;
    crate::GeneratedMap::verify_directory(output_directory, &package.content_hash_hex())?;
    Ok(package)
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
    sampler: &mut super::sampler::Sampler,
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
