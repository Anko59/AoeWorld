use super::*;
use crate::map_jobs::HydrologyMode;

pub(super) fn verify(package: &MapPackage, mode: HydrologyMode) -> Result<(), String> {
    let index = package.environment.hydrology_evidence.as_ref();
    if mode == HydrologyMode::None {
        return if index.is_none() && package.preparation_notes.is_empty() {
            Ok(())
        } else {
            Err("worker returned hydrology for an overview without rivers".to_owned())
        };
    }
    if mode == HydrologyMode::Automatic && index.is_none() {
        return if package.preparation_notes == [aoe_map::PreparationNote::OutsideRiverCoverage]
            && package.source_locks.len() == aoe_map::LANDSCAPE_OVERVIEW_SOURCE_LOCKS
            && !package.source_locks.iter().any(|lock| {
                ["hydrolakes-v1.0-global-gdb", "hydrorivers-v1.0-eu-shp"]
                    .contains(&lock.id.as_str())
            }) {
            Ok(())
        } else {
            Err("automatic worker omitted rivers without an explicit coverage outcome".to_owned())
        };
    }
    let index = index.ok_or("vector worker returned no typed hydrology")?;
    if !package.preparation_notes.is_empty()
        || index.samples_per_axis != aoe_map::LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS
        || index.water_model.is_none()
        || package.source_locks.len() != VECTOR_OVERVIEW_SOURCE_LOCKS
        || ["hydrolakes-v1.0-global-gdb","hydrorivers-v1.0-eu-shp"].iter().any(|id|
            !package.source_locks.iter().any(|lock| lock.id == *id && lock.preprocessing_version.contains(
                "modern-landcover=not-requested;modern-class=0-nodata;2021=classification-legend-only"))) {
        return Err("vector worker returned incompatible hydrology metadata".to_owned());
    }
    Ok(())
}
