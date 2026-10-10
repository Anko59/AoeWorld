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

pub(super) fn preflight(
    mode: OverviewHydrologyMode,
    request: MapRequest,
    samples_per_axis: u16,
    axes: OverviewFieldAxes,
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
    if axes != OverviewFieldAxes::LANDSCAPE {
        return Err(GeodataError::Preparation(
            "overview vectors require the landscape axes with water128",
        ));
    }
    if samples_per_axis != VECTOR_SAMPLES_PER_AXIS {
        return Err(GeodataError::Preparation(
            "overview vectors require the landscape overview samples per axis",
        ));
    }
    let corrections = match corrections {
        Some(document) => document,
        None => WaterCorrectionDocument::empty(request, VECTOR_SAMPLES_PER_AXIS)?,
    };
    PreparedHydrology::validate_vectors_request(
        request,
        VECTOR_SAMPLES_PER_AXIS,
        &corrections,
        cancelled,
    )?;
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
        if environment.samples_per_axis != VECTOR_SAMPLES_PER_AXIS
            || environment.water_samples_per_axis() != Some(128)
            || environment.vegetation_samples_per_axis() != Some(128)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_vector_axis_mismatch_before_acquisition() {
        let request = MapRequest::default();
        let cancelled = AtomicBool::new(false);
        let axes = OverviewFieldAxes::LANDSCAPE;
        let vectors = OverviewHydrologyMode::Vectors;
        assert!(
            preflight(
                vectors,
                request,
                VECTOR_SAMPLES_PER_AXIS,
                axes,
                None,
                &cancelled
            )
            .is_ok()
        );
        assert!(matches!(
            preflight(vectors, request, 128, axes, None, &cancelled),
            Err(GeodataError::Preparation(message)) if message.contains("samples per axis")
        ));
    }

    #[test]
    fn named_lock_counts_match_the_documented_source_lists() {
        assert_eq!(
            crate::overview_sources().unwrap().len(),
            LANDSCAPE_OVERVIEW_SOURCE_LOCKS
        );
        assert_eq!(
            crate::hydrology_vector_sources().len(),
            VECTOR_HYDROLOGY_SOURCE_LOCKS
        );
    }
}
