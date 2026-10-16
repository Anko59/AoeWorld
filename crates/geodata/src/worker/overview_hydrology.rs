use crate::{
    GeneratedMap, GeodataError, OverviewFieldAxes, OverviewHydrologyMode, PreparedHydrology,
};
use aoe_map::{
    LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS, LANDSCAPE_OVERVIEW_SOURCE_LOCKS, MapPackage, MapRequest,
    VECTOR_HYDROLOGY_SOURCE_LOCKS, WaterCorrectionDocument,
};
use std::sync::atomic::AtomicBool;

/// Vector evidence and modeled water share the landscape overview axis.
pub(super) const VECTOR_SAMPLES_PER_AXIS: u16 = LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS;

/// An immutable preflight decision, bound to its request and preparation grid.
/// The CLI can carry this into execution instead of projecting a second time.
#[derive(Clone, Debug)]
pub struct OverviewHydrologyPreflight {
    request: MapRequest,
    mode: OverviewHydrologyMode,
    axis: u16,
    axes: OverviewFieldAxes,
    requested_corrections: Option<WaterCorrectionDocument>,
    corrections: Option<WaterCorrectionDocument>,
    footprint: Option<crate::hydrology::RiverFootprint>,
    outside: bool,
}

impl OverviewHydrologyPreflight {
    pub fn has_vectors(&self) -> bool {
        self.footprint.is_some()
    }
    pub(super) fn validate_binding(
        &self,
        request: MapRequest,
        mode: OverviewHydrologyMode,
        axis: u16,
        axes: OverviewFieldAxes,
        corrections: &Option<WaterCorrectionDocument>,
    ) -> Result<(), GeodataError> {
        if request.normalized().ok() != Some(self.request)
            || mode != self.mode
            || axis != self.axis
            || axes != self.axes
            || corrections != &self.requested_corrections
        {
            return Err(GeodataError::Preparation(
                "overview hydrology preflight does not match execution request",
            ));
        }
        Ok(())
    }
    pub(super) fn prepare(
        &self,
        cache: std::path::PathBuf,
        elevation: &[aoe_map::ElevationPage],
        water: &[aoe_map::WaterPage],
        cancelled: &AtomicBool,
    ) -> Result<Option<PreparedHydrology>, GeodataError> {
        self.footprint
            .clone()
            .map(|footprint| {
                crate::hydrology::prepare_vectors_validated(
                    cache,
                    footprint,
                    elevation,
                    water,
                    self.corrections.clone().ok_or(GeodataError::Preparation(
                        "vector preflight has no correction binding",
                    ))?,
                    cancelled,
                )
            })
            .transpose()
    }
    pub(super) fn mark_package(&self, package: MapPackage) -> Result<MapPackage, GeodataError> {
        if package.request != self.request
            || (self.has_vectors() && package.environment.hydrology_evidence.is_none())
        {
            return Err(GeodataError::Preparation(
                "hydrology result does not match preflight",
            ));
        }
        Ok(if self.outside {
            package.with_preparation_note(aoe_map::PreparationNote::OutsideRiverCoverage)?
        } else {
            package
        })
    }
}

pub fn preflight(
    mode: OverviewHydrologyMode,
    request: MapRequest,
    samples_per_axis: u16,
    axes: OverviewFieldAxes,
    corrections: Option<WaterCorrectionDocument>,
    cancelled: &AtomicBool,
) -> Result<OverviewHydrologyPreflight, GeodataError> {
    if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::CacheError::Cancelled.into());
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let mut plan = OverviewHydrologyPreflight {
        request,
        mode,
        axis: samples_per_axis,
        axes,
        requested_corrections: corrections.clone(),
        corrections: None,
        footprint: None,
        outside: false,
    };
    if mode == OverviewHydrologyMode::None {
        if corrections.is_some() {
            return Err(GeodataError::Preparation(
                "overview water corrections require vectors mode",
            ));
        }
        return Ok(plan);
    }
    if axes != OverviewFieldAxes::LANDSCAPE {
        return Err(GeodataError::Preparation(
            "overview vectors require the landscape axes",
        ));
    }
    if samples_per_axis != VECTOR_SAMPLES_PER_AXIS {
        return Err(GeodataError::Preparation(
            "overview vectors require the landscape overview samples per axis",
        ));
    }
    let document = match corrections {
        Some(document) => document,
        None => WaterCorrectionDocument::empty(request, VECTOR_SAMPLES_PER_AXIS)?,
    };
    document.validate_for(request, VECTOR_SAMPLES_PER_AXIS)?;
    let footprint =
        crate::hydrology::project_river_footprint(request, VECTOR_SAMPLES_PER_AXIS, cancelled)?;
    if !footprint.covered()
        && mode == OverviewHydrologyMode::Automatic
        && plan.requested_corrections.is_none()
    {
        plan.outside = true;
    } else {
        footprint.check_binding(request, VECTOR_SAMPLES_PER_AXIS)?;
        plan.footprint = Some(footprint);
        plan.corrections = Some(document);
    }
    Ok(plan)
}

impl GeneratedMap {
    /// Adds verified vector water and removes coarse HYDE inland coverage.
    /// Overview ocean and vegetation share the 1024 landscape axis.
    pub fn with_overview_vector_hydrology(
        mut self,
        hydrology: PreparedHydrology,
    ) -> Result<Self, GeodataError> {
        self.validate()?;
        let environment = &self.package.environment;
        if environment.samples_per_axis != VECTOR_SAMPLES_PER_AXIS
            || environment.water_samples_per_axis() != Some(VECTOR_SAMPLES_PER_AXIS)
            || environment.vegetation_samples_per_axis() != Some(VECTOR_SAMPLES_PER_AXIS)
            || environment.historical_samples_per_axis() != Some(VECTOR_SAMPLES_PER_AXIS)
            || environment.hydrology_evidence.is_some()
            || hydrology.samples_per_axis != VECTOR_SAMPLES_PER_AXIS
            || hydrology.evidence_index.water_model.is_none()
            || hydrology.modern_land_cover_preprocessing().is_none()
            || hydrology.source_locks.len() != VECTOR_HYDROLOGY_SOURCE_LOCKS
            || self.package.source_locks.len() != LANDSCAPE_OVERVIEW_SOURCE_LOCKS
        {
            return Err(GeodataError::Preparation(
                "invalid overview vector composition",
            ));
        }
        for source in crate::hydrology_vector_sources() {
            let expected = source
                .cache_lock()
                .ok_or(GeodataError::Preparation("vector source is not pinned"))?;
            if !hydrology.source_locks.iter().any(|lock| {
                lock.id == expected.id
                    && crate::source_cache::digest_hex(&lock.sha256) == expected.sha256
            }) {
                return Err(GeodataError::Preparation(
                    "vector source lock does not match pinned input",
                ));
            }
        }
        hydrology.evidence_index.validate_pages(
            &hydrology.hydrology_pages,
            &hydrology.modern_land_cover_pages,
            self.package.estimate.tiles_per_side,
        )?;
        let marker =
            hydrology
                .modern_land_cover_preprocessing()
                .ok_or(GeodataError::Preparation(
                    "vector-only modern metadata marker is missing",
                ))?;
        if hydrology.hydrology_pages.iter().any(|page| {
            page.method
                .contains(&(aoe_map::HydrologyEvidenceMethod::WorldCoverClass as u8))
        }) || hydrology
            .source_locks
            .iter()
            .any(|lock| !lock.preprocessing_version.contains(marker))
            || hydrology
                .modern_land_cover_pages
                .iter()
                .any(|page| page.worldcover_class.iter().any(|class| *class != 0))
        {
            return Err(GeodataError::Preparation(
                "vector-only modern land cover must be nodata",
            ));
        }
        let mut environment = self.package.environment.clone();
        for page in &mut self.water_pages {
            page.inland_coverage_percent.fill(0);
        }
        let water = environment.water.as_mut().ok_or(GeodataError::Preparation(
            "overview vector composition has no ocean pyramid",
        ))?;
        for (level, index) in water.levels.iter_mut().enumerate() {
            let pages = self
                .water_pages
                .iter()
                .filter(|page| usize::from(page.level) == level)
                .cloned()
                .collect::<Vec<_>>();
            index.ordered_page_root = aoe_map::ordered_water_page_root(&pages)?;
        }
        environment.hydrology_evidence = Some(hydrology.evidence_index);
        let mut locks = self.package.source_locks.clone();
        locks.extend(hydrology.source_locks);
        self.package = MapPackage::with_prepared_environment(
            self.package.generator_version,
            self.package.request,
            locks,
            self.package.projection.clone(),
            self.package.provenance.clone(),
            environment,
        )?;
        self.hydrology_evidence_pages = hydrology.hydrology_pages;
        self.modern_land_cover_pages = hydrology.modern_land_cover_pages;
        self.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
#[path = "tests/overview_hydrology.rs"]
mod tests;
