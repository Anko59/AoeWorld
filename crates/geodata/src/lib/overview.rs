//! Shared overview preparation with independent correction layers.
use super::*;
#[path = "overview/axes.rs"]
mod axes;
pub use axes::OverviewFieldAxes;
pub(crate) use axes::validate_field_axes;

pub fn prepare_overview(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_with_corrections(cache_root, request, samples_per_axis, None)
}

pub fn prepare_overview_with_corrections(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    historical_corrections: Option<&GeographicHistoricalCorrectionDocument>,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_with_historical_axis(
        cache_root,
        request,
        samples_per_axis,
        samples_per_axis,
        historical_corrections,
    )
}

/// Detailed preparation retains its 128-sample elevation overview while
/// sampling history on an independent, bounded grid.
pub fn prepare_overview_with_historical_axis(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    historical_samples_per_axis: u16,
    historical_corrections: Option<&GeographicHistoricalCorrectionDocument>,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_with_all_corrections(
        cache_root,
        request,
        samples_per_axis,
        historical_samples_per_axis,
        historical_corrections,
        None,
    )
}

pub fn prepare_overview_with_vegetation_corrections(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    vegetation_corrections: Option<&VegetationPatchDocument>,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_with_all_corrections(
        cache_root,
        request,
        samples_per_axis,
        samples_per_axis,
        None,
        vegetation_corrections,
    )
}

pub fn prepare_overview_with_all_corrections(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    historical_samples_per_axis: u16,
    historical_corrections: Option<&GeographicHistoricalCorrectionDocument>,
    vegetation_corrections: Option<&VegetationPatchDocument>,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_axes(
        cache_root,
        request,
        OverviewFieldAxes::coupled(samples_per_axis, historical_samples_per_axis),
        false,
        historical_corrections,
        vegetation_corrections,
    )
}

/// Bounded field-local preparation of an overview package.
pub fn prepare_overview_with_field_axes(
    cache_root: PathBuf,
    request: MapRequest,
    axes: OverviewFieldAxes,
    historical_corrections: Option<&GeographicHistoricalCorrectionDocument>,
    vegetation_corrections: Option<&VegetationPatchDocument>,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_axes(
        cache_root,
        request,
        axes,
        true,
        historical_corrections,
        vegetation_corrections,
    )
}

fn prepare_overview_axes(
    cache_root: PathBuf,
    request: MapRequest,
    axes: OverviewFieldAxes,
    explicit: bool,
    historical_corrections: Option<&GeographicHistoricalCorrectionDocument>,
    vegetation_corrections: Option<&VegetationPatchDocument>,
) -> Result<PreparedOverview, GeodataError> {
    validate_field_axes(axes, explicit)?;
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let historical_corrections = match historical_corrections {
        Some(document) => std::borrow::Cow::Borrowed(document),
        None => std::borrow::Cow::Owned(GeographicHistoricalCorrectionDocument::empty(
            request,
            axes.historical,
            hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
        )?),
    };
    historical_corrections.validate_for(
        request,
        axes.historical,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )?;
    let vegetation_corrections = match vegetation_corrections {
        Some(document) => std::borrow::Cow::Borrowed(document),
        None => std::borrow::Cow::Owned(VegetationPatchDocument::empty(request, axes.vegetation)?),
    };
    vegetation_corrections.validate_for(request, axes.vegetation)?;
    let source = etopo_2022_60s_surface();
    let lock = source
        .cache_lock()
        .ok_or(GeodataError::Preparation("overview source lacks SHA-256"))?;
    let water_source = natural_earth_10m_land();
    let water_lock = water_source
        .cache_lock()
        .ok_or(GeodataError::Preparation("coastline source lacks SHA-256"))?;
    let cache = SourceCache::new(
        cache_root,
        DownloadPolicy {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: MAX_OVERVIEW_INPUT_BYTES,
        },
    )?;
    let potential_sources = potential_biome_sources().ok();
    let hyde_sources = hyde_sources().ok();
    preflight_overview_acquisition(
        &cache,
        potential_sources.as_deref(),
        hyde_sources.as_deref(),
    )?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let path = cache.acquire(&lock, &cancelled)?;
    let water_path = cache.acquire(&water_lock, &cancelled)?;
    let vegetation_lock = acquire_or_cached(
        &cache,
        potential_sources.as_deref(),
        POTENTIAL_BIOME_RASTER_ID,
        &cancelled,
    )?;
    let vegetation_classes_lock = acquire_or_cached(
        &cache,
        potential_sources.as_deref(),
        POTENTIAL_BIOME_CLASSES_ID,
        &cancelled,
    )?;
    let vegetation_path = cache.object_path(&vegetation_lock)?;
    let vegetation_classes_path = cache.object_path(&vegetation_classes_lock)?;
    let hyde_baseline_lock = acquire_or_cached(
        &cache,
        hyde_sources.as_deref(),
        HYDE_BASELINE_ID,
        &cancelled,
    )?;
    let hyde_supplementary_lock = acquire_or_cached(
        &cache,
        hyde_sources.as_deref(),
        HYDE_SUPPLEMENTARY_ID,
        &cancelled,
    )?;
    let hyde_readme_lock =
        acquire_or_cached(&cache, hyde_sources.as_deref(), HYDE_README_ID, &cancelled)?;
    let hyde_baseline_path = cache.object_path(&hyde_baseline_lock)?;
    let hyde_supplementary_path = cache.object_path(&hyde_supplementary_lock)?;
    prepare_overview_fields_from_verified_sources(
        request,
        axes,
        explicit,
        &historical_corrections,
        &vegetation_corrections,
        VerifiedOverviewSources {
            elevation: VerifiedOverviewSource { lock, path },
            water: VerifiedOverviewSource {
                lock: water_lock,
                path: water_path,
            },
            vegetation: VerifiedOverviewSource {
                lock: vegetation_lock,
                path: vegetation_path,
            },
            vegetation_classes: VerifiedOverviewSource {
                lock: vegetation_classes_lock,
                path: vegetation_classes_path,
            },
            hyde_baseline: VerifiedOverviewSource {
                lock: hyde_baseline_lock,
                path: hyde_baseline_path,
            },
            hyde_supplementary: VerifiedOverviewSource {
                lock: hyde_supplementary_lock,
                path: hyde_supplementary_path,
            },
            hyde_readme: hyde_readme_lock,
        },
    )
}

#[derive(Clone)]
struct VerifiedOverviewSource {
    lock: SourceLock,
    path: PathBuf,
}

#[derive(Clone)]
struct VerifiedOverviewSources {
    elevation: VerifiedOverviewSource,
    water: VerifiedOverviewSource,
    vegetation: VerifiedOverviewSource,
    vegetation_classes: VerifiedOverviewSource,
    hyde_baseline: VerifiedOverviewSource,
    hyde_supplementary: VerifiedOverviewSource,
    hyde_readme: SourceLock,
}

/// Samples cache-verified overview inputs without performing source discovery
/// or acquisition. The public entry point owns those verification steps.
#[cfg(test)]
fn prepare_overview_from_verified_sources(
    request: MapRequest,
    samples_per_axis: u16,
    historical_samples_per_axis: u16,
    historical_corrections: &GeographicHistoricalCorrectionDocument,
    vegetation_corrections: &VegetationPatchDocument,
    sources: VerifiedOverviewSources,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_fields_from_verified_sources(
        request,
        OverviewFieldAxes::coupled(samples_per_axis, historical_samples_per_axis),
        false,
        historical_corrections,
        vegetation_corrections,
        sources,
    )
}

fn prepare_overview_fields_from_verified_sources(
    request: MapRequest,
    axes: OverviewFieldAxes,
    explicit: bool,
    historical_corrections: &GeographicHistoricalCorrectionDocument,
    vegetation_corrections: &VegetationPatchDocument,
    sources: VerifiedOverviewSources,
) -> Result<PreparedOverview, GeodataError> {
    validate_field_axes(axes, explicit)?;
    historical_corrections.validate_for(
        request,
        axes.historical,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )?;
    vegetation_corrections.validate_for(request, axes.vegetation)?;
    let total_pages = [axes.elevation, axes.water, axes.vegetation, axes.historical]
        .into_iter()
        .map(preparation_progress::pyramid_page_count)
        .sum();
    let mut completed_pages = 0;
    preparation_progress::stage(preparation_progress::Phase::SamplingOverview);
    let mut prepared = if explicit {
        elevation::prepare_landscape_elevation(&sources.elevation.path, request, axes.elevation)?
    } else {
        prepare_elevation(&sources.elevation.path, request, axes.elevation)?
    };
    completed_pages += prepared.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let lake_coverage =
        prepare_hyde_lake_coverage(&sources.hyde_supplementary.path, request, axes.water)?;
    let water = prepare_ocean_coverage(&sources.water.path, request, axes.water, lake_coverage)?;
    completed_pages += water.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let vegetation = prepare_potential_biomes_with_corrections(
        &sources.vegetation.path,
        request,
        axes.vegetation,
        vegetation_corrections,
    )?;
    completed_pages += vegetation.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let historical_preprocessing = preprocessing_identity(
        format!(
            "{};corrections-sha256={}",
            hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
            historical_corrections.canonical_digest_hex(request)?
        ),
        axes,
        explicit,
    );
    let historical_land_use = prepare_hyde_area_600_with_corrections(
        &sources.hyde_baseline.path,
        &sources.hyde_supplementary.path,
        request,
        axes.historical,
        historical_corrections,
    )?;
    completed_pages += historical_land_use.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    verify_potential_biome_legend(&sources.vegetation_classes.path)?;
    prepared.environment.water = Some(water.field);
    prepared.environment.vegetation = Some(vegetation.field);
    prepared.environment.historical_land_use = Some(historical_land_use.field);
    prepared.environment.validate()?;
    Ok(PreparedOverview {
        source_lock: sources.elevation.lock.to_map_source_lock(
            acquisition_marker(),
            preprocessing_identity("etopo-overview-gdal-0.19".to_owned(), axes, explicit),
        )?,
        water_source_lock: sources.water.lock.to_map_source_lock(
            acquisition_marker(),
            preprocessing_identity(
                "natural-earth-coastline-gdal-0.19".to_owned(),
                axes,
                explicit,
            ),
        )?,
        vegetation_source_lock: sources.vegetation.lock.to_map_source_lock(
            acquisition_marker(),
            preprocessing_identity(
                format!(
                    "{};corrections-sha256={}",
                    VEGETATION_PATCH_PREPROCESSING_IDENTITY,
                    vegetation_corrections.digest_hex(request)?
                ),
                axes,
                explicit,
            ),
        )?,
        vegetation_classes_source_lock: sources.vegetation_classes.lock.to_map_source_lock(
            acquisition_marker(),
            "potential-biome-class-legend-v0.2".to_owned(),
        )?,
        hyde_baseline_source_lock: sources
            .hyde_baseline
            .lock
            .to_map_source_lock(acquisition_marker(), historical_preprocessing.clone())?,
        hyde_supplementary_source_lock: sources
            .hyde_supplementary
            .lock
            .to_map_source_lock(acquisition_marker(), historical_preprocessing)?,
        hyde_readme_source_lock: sources.hyde_readme.to_map_source_lock(
            acquisition_marker(),
            "hyde-3.2.1-release-notes-v1".to_owned(),
        )?,
        projection: ProjectionMetadata {
            horizontal_crs: local_aeqd_definition(
                request.center_latitude_e7,
                request.center_longitude_e7,
            ),
            vertical_datum: VerticalDatum::Egm2008Orthometric,
            tool_version: "GDAL Rust bindings 0.19 / PROJ native".to_owned(),
        },
        provenance: EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: LayerProvenance::SourceDerived,
            vegetation: if vegetation_corrections.changes_historical_vegetation() {
                LayerProvenance::HistoricallyCorrected
            } else {
                LayerProvenance::SourceDerived
            },
            historical_land_use: if historical_corrections.changes_historical_land_use() {
                LayerProvenance::HistoricallyCorrected
            } else {
                LayerProvenance::SourceDerived
            },
        },
        environment: prepared.environment,
        pages: prepared.pages,
        water_pages: water.pages,
        vegetation_pages: vegetation.pages,
        historical_land_use_pages: historical_land_use.pages,
    })
}

fn preprocessing_identity(base: String, axes: OverviewFieldAxes, explicit: bool) -> String {
    if !explicit {
        return base;
    }
    format!(
        "{base};axes={}/{}/{}/{}",
        axes.elevation, axes.vegetation, axes.water, axes.historical
    )
}

#[path = "overview/tests.rs"]
#[cfg(test)]
mod tests;
