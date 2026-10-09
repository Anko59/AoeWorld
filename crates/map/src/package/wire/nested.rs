use super::*;
use crate::{
    DetailProfile, FieldPyramid, GeographicWaterPatch, HydrologyEvidenceIndex,
    HydrologyWaterModelIndex, HydrologyWaterPolicy, PyramidLevel, Ratio, ReconstructionProfile,
    WaterCorrectionDocument, WaterCorrectionProjection,
};

strict_object!(StrictRatio, ratio, Ratio, {numerator: u32, denominator: u32});
strict_object!(StrictRequest, request, MapRequest, {
    schema_version: u16, center_latitude_e7: i32, center_longitude_e7: i32,
    requested_side_meters: u64,
    #[serde(deserialize_with = "ratio")]
    compression: Ratio,
    year_ce: u16, seed: u64,
    #[serde(default)]
    reconstruction_profile: ReconstructionProfile,
    #[serde(default)]
    detail_profile: DetailProfile,
});
strict_object!(StrictEstimate, estimate, MapEstimate, {
    effective_side_meters: u64, tiles_per_side: u64, game_side_meters: u64,
    geographic_millimeters_per_tile: u64, walking_crossing_seconds: u64,
    cavalry_crossing_seconds: u64,
});
strict_object!(StrictProjection, projection, ProjectionMetadata, {
    horizontal_crs: String, vertical_datum: VerticalDatum, tool_version: String,
});
strict_object!(StrictProvenance, provenance, EnvironmentalProvenance, {
    elevation: LayerProvenance, water: LayerProvenance, vegetation: LayerProvenance,
    historical_land_use: LayerProvenance,
});
strict_object!(StrictSource, SourceLock, {
    id: String, provider: String, release: String, url: String, sha256: [u8; 32],
    acquired_at: String, native_resolution: String, crs: String, vertical_datum: String,
    license: String, preprocessing_version: String,
});
strict_object!(StrictLevel, PyramidLevel, {
    samples_per_axis: u16, ordered_page_root: [u8; 32],
});
strict_object!(StrictPyramid, pyramid, FieldPyramid, {
    #[serde(deserialize_with = "levels")]
    levels: Vec<PyramidLevel>,
});
strict_object!(StrictHydrology, HydrologyEvidenceIndex, {
    samples_per_axis: u16, page_samples: u8, world_cover_year: u16,
    policy: HydrologyWaterPolicy, hydrology_page_root: [u8; 32],
    modern_land_cover_page_root: [u8; 32],
    #[serde(default, deserialize_with = "optional_model")]
    water_model: Option<HydrologyWaterModelIndex>,
});
strict_object!(StrictModel, HydrologyWaterModelIndex, {
    model_version: u16, samples_per_axis: u16, target_year_ce: u16,
    #[serde(deserialize_with = "correction")]
    correction_document: WaterCorrectionDocument,
});
strict_object!(StrictCorrection, correction, WaterCorrectionDocument, {
    schema_version: u16, target_year_ce: u16, samples_per_axis: u16,
    #[serde(deserialize_with = "request")]
    request: MapRequest,
    projection: WaterCorrectionProjection,
    patches: Vec<GeographicWaterPatch>,
});
strict_object!(StrictEnvironment, environment, PreparedEnvironment, {
    samples_per_axis: u16, geographic_millimeters_per_sample: u64, page_samples: u8,
    #[serde(deserialize_with = "pyramid")]
    elevation: FieldPyramid,
    #[serde(default, deserialize_with = "optional_pyramid")]
    water: Option<FieldPyramid>,
    #[serde(default, deserialize_with = "optional_pyramid")]
    vegetation: Option<FieldPyramid>,
    #[serde(default, deserialize_with = "optional_pyramid")]
    historical_land_use: Option<FieldPyramid>,
    #[serde(default, deserialize_with = "optional_hydrology")]
    hydrology_evidence: Option<HydrologyEvidenceIndex>,
});

pub(super) fn source_locks<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SourceLock>, D::Error> {
    Vec::<StrictSource>::deserialize(deserializer)
        .map(|values| values.into_iter().map(Into::into).collect())
}
fn levels<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PyramidLevel>, D::Error> {
    Vec::<StrictLevel>::deserialize(deserializer)
        .map(|values| values.into_iter().map(Into::into).collect())
}
fn optional_pyramid<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<FieldPyramid>, D::Error> {
    Option::<StrictPyramid>::deserialize(deserializer).map(|value| value.map(Into::into))
}
fn optional_model<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<HydrologyWaterModelIndex>, D::Error> {
    Option::<StrictModel>::deserialize(deserializer).map(|value| value.map(Into::into))
}
fn optional_hydrology<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<HydrologyEvidenceIndex>, D::Error> {
    Option::<StrictHydrology>::deserialize(deserializer).map(|value| value.map(Into::into))
}
