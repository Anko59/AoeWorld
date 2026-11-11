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
            && aoe_map::overview_base_lock_count(&package.source_locks)
                == aoe_map::LANDSCAPE_OVERVIEW_SOURCE_LOCKS
            && !package
                .source_locks
                .iter()
                .any(|lock| aoe_map::is_vector_hydrology_lock(&lock.id))
        {
            Ok(())
        } else {
            Err("automatic worker omitted rivers without an explicit coverage outcome".to_owned())
        };
    }
    let index = index.ok_or("vector worker returned no typed hydrology")?;
    if !package.preparation_notes.is_empty()
        || index.samples_per_axis != aoe_map::LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS
        || index.water_model.is_none()
        || aoe_map::overview_base_lock_count(&package.source_locks) != VECTOR_OVERVIEW_SOURCE_LOCKS
        || !vector_locks_are_marked(package)
    {
        return Err("vector worker returned incompatible hydrology metadata".to_owned());
    }
    Ok(())
}

/// HydroLAKES and exactly one regional HydroRIVERS lock, both from vector-only
/// preparation (modern land cover not requested).
fn vector_locks_are_marked(package: &MapPackage) -> bool {
    const MARKER: &str =
        "modern-landcover=not-requested;modern-class=0-nodata;2021=classification-legend-only";
    let marked = |lock: &&aoe_map::SourceLock| lock.preprocessing_version.contains(MARKER);
    let lakes = package
        .source_locks
        .iter()
        .filter(|lock| lock.id == aoe_map::HYDROLAKES_SOURCE_ID)
        .filter(marked)
        .count();
    let rivers = package
        .source_locks
        .iter()
        .filter(|lock| aoe_map::is_hydrorivers_lock(&lock.id))
        .collect::<Vec<_>>();
    lakes == 1 && rivers.len() == 1 && rivers.iter().all(marked)
}
