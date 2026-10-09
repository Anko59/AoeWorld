//! Explicit opt-in contracts, independent of published default-version aliases.
use crate::{DetailProfile, MapPackageError};

pub(super) fn schema(detail: DetailProfile) -> u16 {
    match detail {
        DetailProfile::StandardV1 => crate::MAP_SCHEMA_VERSION,
        DetailProfile::LandscapeV2 => crate::LANDSCAPE_MAP_SCHEMA_VERSION,
    }
}

pub(super) fn validate(
    schema: u16,
    detail: DetailProfile,
    recipe: u16,
    has_typed_evidence: bool,
) -> Result<(), MapPackageError> {
    let valid = match detail {
        DetailProfile::StandardV1 => {
            let schema_valid = schema == crate::MAP_SCHEMA_VERSION
                || (schema == crate::LEGACY_MAP_SCHEMA_VERSION && !has_typed_evidence);
            // Published readers rejected the schema first, then an unsupported
            // recipe with this specific error. Preserve that legacy precedence.
            if schema_valid && !matches!(recipe, 3..=8) {
                return Err(MapPackageError::InvalidGenerationRecipeVersion);
            }
            schema_valid
        }
        DetailProfile::LandscapeV2 => {
            schema == crate::LANDSCAPE_MAP_SCHEMA_VERSION
                && recipe == crate::LANDSCAPE_GENERATION_RECIPE_VERSION
        }
    };
    valid
        .then_some(())
        .ok_or(MapPackageError::NonCanonicalFields)
}
