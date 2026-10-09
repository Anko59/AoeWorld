use crate::{
    GeneratedMap, GeodataError, OverviewFieldAxes, OverviewHydrologyMode, PreparedHydrology,
};
use aoe_map::{DetailProfile, MapPackage, MapRequest, WaterCorrectionDocument};
use std::sync::atomic::AtomicBool;

pub(super) fn preflight(
    mode: OverviewHydrologyMode,
    request: MapRequest,
    axes: Option<OverviewFieldAxes>,
    corrections: Option<WaterCorrectionDocument>,
    cancelled: &AtomicBool,
) -> Result<Option<WaterCorrectionDocument>, GeodataError> {
    if mode == OverviewHydrologyMode::None {
        if corrections.is_some() {
            return Err(GeodataError::Preparation(
                "overview water corrections require vectors mode",
            ));
        }
        return Ok(None);
    }
    if request.detail_profile != DetailProfile::LandscapeV2
        || axes != Some(OverviewFieldAxes::LANDSCAPE)
    {
        return Err(GeodataError::Preparation(
            "overview vectors require LandscapeV2 landscape axes with water128",
        ));
    }
    let corrections = match corrections {
        Some(document) => document,
        None => WaterCorrectionDocument::empty(request, 1024)?,
    };
    PreparedHydrology::validate_vectors_request(request, 1024, &corrections, cancelled)?;
    Ok(Some(corrections))
}

impl GeneratedMap {
    /// Adds verified typed vector evidence/model without replacing any overview
    /// pyramid. Categorical water stays at 128; modeled water has its own 1024
    /// axis, consumed independently by eager and provider-backed terrain APIs.
    pub fn with_overview_vector_hydrology(
        mut self,
        hydrology: PreparedHydrology,
    ) -> Result<Self, GeodataError> {
        self.validate()?;
        let environment = &self.package.environment;
        if self.package.request.detail_profile != DetailProfile::LandscapeV2
            || environment.samples_per_axis != 1024
            || environment.water_samples_per_axis() != Some(128)
            || environment.vegetation_samples_per_axis() != Some(128)
            || environment.historical_samples_per_axis() != Some(1024)
            || environment.hydrology_evidence.is_some()
            || hydrology.samples_per_axis != 1024
            || hydrology.evidence_index.water_model.is_none()
            || hydrology.modern_land_cover_preprocessing().is_none()
            || hydrology.source_locks.len() != 2
            || self.package.source_locks.len() != 7
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
