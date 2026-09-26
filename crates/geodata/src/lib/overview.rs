//! Shared overview preparation with independent correction layers.
use super::*;

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
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let historical_corrections = match historical_corrections {
        Some(document) => std::borrow::Cow::Borrowed(document),
        None => std::borrow::Cow::Owned(GeographicHistoricalCorrectionDocument::empty(
            request,
            historical_samples_per_axis,
            hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
        )?),
    };
    historical_corrections.validate_for(
        request,
        historical_samples_per_axis,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )?;
    let vegetation_corrections = match vegetation_corrections {
        Some(document) => std::borrow::Cow::Borrowed(document),
        None => std::borrow::Cow::Owned(VegetationPatchDocument::empty(request, samples_per_axis)?),
    };
    vegetation_corrections.validate_for(request, samples_per_axis)?;
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
    let total_pages = preparation_progress::pyramid_page_count(samples_per_axis) * 3
        + preparation_progress::pyramid_page_count(historical_samples_per_axis);
    let mut completed_pages = 0;
    preparation_progress::stage(preparation_progress::Phase::SamplingOverview);
    let mut prepared = prepare_elevation(&path, request, samples_per_axis)?;
    completed_pages += prepared.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let lake_coverage =
        prepare_hyde_lake_coverage(&hyde_supplementary_path, request, samples_per_axis)?;
    let water = prepare_ocean_coverage(&water_path, request, samples_per_axis, lake_coverage)?;
    completed_pages += water.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let vegetation = prepare_potential_biomes_with_corrections(
        &vegetation_path,
        request,
        samples_per_axis,
        &vegetation_corrections,
    )?;
    completed_pages += vegetation.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let historical_preprocessing = format!(
        "{};corrections-sha256={}",
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
        historical_corrections.canonical_digest_hex(request)?
    );
    let historical_land_use = prepare_hyde_area_600_with_corrections(
        &hyde_baseline_path,
        &hyde_supplementary_path,
        request,
        historical_samples_per_axis,
        &historical_corrections,
    )?;
    completed_pages += historical_land_use.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    verify_potential_biome_legend(&vegetation_classes_path)?;
    prepared.environment.water = Some(water.field);
    prepared.environment.vegetation = Some(vegetation.field);
    prepared.environment.historical_land_use = Some(historical_land_use.field);
    prepared.environment.validate()?;
    Ok(PreparedOverview {
        source_lock: lock
            .to_map_source_lock(acquisition_marker(), "etopo-overview-gdal-0.19".to_owned())?,
        water_source_lock: water_lock.to_map_source_lock(
            acquisition_marker(),
            "natural-earth-coastline-gdal-0.19".to_owned(),
        )?,
        vegetation_source_lock: vegetation_lock.to_map_source_lock(
            acquisition_marker(),
            format!(
                "{};corrections-sha256={}",
                VEGETATION_PATCH_PREPROCESSING_IDENTITY,
                vegetation_corrections.digest_hex(request)?
            ),
        )?,
        vegetation_classes_source_lock: vegetation_classes_lock.to_map_source_lock(
            acquisition_marker(),
            "potential-biome-class-legend-v0.2".to_owned(),
        )?,
        hyde_baseline_source_lock: hyde_baseline_lock
            .to_map_source_lock(acquisition_marker(), historical_preprocessing.clone())?,
        hyde_supplementary_source_lock: hyde_supplementary_lock
            .to_map_source_lock(acquisition_marker(), historical_preprocessing)?,
        hyde_readme_source_lock: hyde_readme_lock.to_map_source_lock(
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
