use crate::{GeodataError, PreparedHydrology, copernicus::Stage};
use aoe_map::PageLayer;

pub(super) fn store_hydrology_evidence(
    stage: &Stage,
    prepared: &PreparedHydrology,
) -> Result<(), GeodataError> {
    prepared
        .evidence_index
        .validate_pages(&prepared.hydrology_pages, &prepared.modern_land_cover_pages)?;
    for page in &prepared.hydrology_pages {
        let bytes =
            serde_json::to_vec(page).map_err(|error| GeodataError::Directory(error.to_string()))?;
        stage.write(
            PageLayer::HydrologyEvidence,
            page.level,
            page.x,
            page.y,
            &bytes,
        )?;
    }
    for page in &prepared.modern_land_cover_pages {
        let bytes =
            serde_json::to_vec(page).map_err(|error| GeodataError::Directory(error.to_string()))?;
        stage.write(
            PageLayer::ModernLandCover,
            page.level,
            page.x,
            page.y,
            &bytes,
        )?;
    }
    Ok(())
}
