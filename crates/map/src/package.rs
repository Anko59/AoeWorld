use crate::{
    CHUNK_TILES, GENERATION_RECIPE_VERSION, LEGACY_GENERATION_RECIPE_VERSION, MapChunkGenerator,
    MapEstimate, MapRequest, MapRequestError, PRIOR_GENERATION_RECIPE_VERSION, PreparedEnvironment,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionMetadata {
    pub horizontal_crs: String,
    pub vertical_datum: VerticalDatum,
    /// GDAL/PROJ version used to prepare the frozen environmental fields.
    pub tool_version: String,
}

impl Default for ProjectionMetadata {
    fn default() -> Self {
        Self {
            horizontal_crs: "fallback-local-grid-v1".to_owned(),
            vertical_datum: VerticalDatum::UnspecifiedFallback,
            tool_version: "unavailable-fallback".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalDatum {
    Egm2008Orthometric,
    UnspecifiedFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerProvenance {
    SourceDerived,
    ModelDerived,
    Procedural,
    Fallback,
    HistoricallyCorrected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentalProvenance {
    pub elevation: LayerProvenance,
    pub water: LayerProvenance,
    pub vegetation: LayerProvenance,
    pub historical_land_use: LayerProvenance,
}

impl Default for EnvironmentalProvenance {
    fn default() -> Self {
        Self {
            elevation: LayerProvenance::Fallback,
            water: LayerProvenance::Fallback,
            vegetation: LayerProvenance::Procedural,
            historical_land_use: LayerProvenance::Fallback,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceLock {
    pub id: String,
    pub provider: String,
    pub release: String,
    pub url: String,
    pub sha256: [u8; 32],
    /// Informational acquisition time, excluded from canonical map identity.
    pub acquired_at: String,
    pub native_resolution: String,
    pub crs: String,
    pub vertical_datum: String,
    pub license: String,
    pub preprocessing_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapPackage {
    pub schema_version: u16,
    pub generator_version: u16,
    /// Runtime generation behavior used by this immutable package. Missing
    /// values deserialize as recipe 3 so existing schema-8 hashes remain valid.
    #[serde(
        default = "legacy_generation_recipe_version",
        skip_serializing_if = "is_legacy_generation_recipe_version"
    )]
    pub generation_recipe_version: u16,
    pub request: MapRequest,
    pub estimate: MapEstimate,
    pub source_locks: Vec<SourceLock>,
    pub projection: ProjectionMetadata,
    pub provenance: EnvironmentalProvenance,
    pub environment: PreparedEnvironment,
    pub content_hash: [u8; 32],
}

impl MapPackage {
    pub fn new(
        generator_version: u16,
        request: MapRequest,
        source_locks: Vec<SourceLock>,
    ) -> Result<Self, MapPackageError> {
        Self::with_projection(
            generator_version,
            request,
            source_locks,
            ProjectionMetadata::default(),
        )
    }

    pub fn with_projection(
        generator_version: u16,
        request: MapRequest,
        source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
    ) -> Result<Self, MapPackageError> {
        Self::with_environment(
            generator_version,
            request,
            source_locks,
            projection,
            EnvironmentalProvenance::default(),
        )
    }

    pub fn with_environment(
        generator_version: u16,
        request: MapRequest,
        source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
        provenance: EnvironmentalProvenance,
    ) -> Result<Self, MapPackageError> {
        Self::with_prepared_environment(
            generator_version,
            request,
            source_locks,
            projection,
            provenance,
            PreparedEnvironment::default(),
        )
    }

    pub fn with_prepared_environment(
        generator_version: u16,
        request: MapRequest,
        source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
        provenance: EnvironmentalProvenance,
        environment: PreparedEnvironment,
    ) -> Result<Self, MapPackageError> {
        let generation_recipe_version = if environment
            .hydrology_evidence
            .as_ref()
            .and_then(|index| index.water_model.as_ref())
            .is_some()
        {
            crate::WATER_MODEL_GENERATION_RECIPE_VERSION
        } else {
            GENERATION_RECIPE_VERSION
        };
        Self::with_generation_recipe(
            generator_version,
            generation_recipe_version,
            request,
            source_locks,
            projection,
            provenance,
            environment,
        )
    }

    fn with_generation_recipe(
        generator_version: u16,
        generation_recipe_version: u16,
        request: MapRequest,
        mut source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
        provenance: EnvironmentalProvenance,
        environment: PreparedEnvironment,
    ) -> Result<Self, MapPackageError> {
        if !matches!(
            generation_recipe_version,
            LEGACY_GENERATION_RECIPE_VERSION
                | PRIOR_GENERATION_RECIPE_VERSION
                | GENERATION_RECIPE_VERSION
                | crate::WATER_MODEL_GENERATION_RECIPE_VERSION
        ) {
            return Err(MapPackageError::InvalidGenerationRecipeVersion);
        }
        let has_modeled_water = environment
            .hydrology_evidence
            .as_ref()
            .and_then(|index| index.water_model.as_ref())
            .is_some();
        if has_modeled_water
            != (generation_recipe_version == crate::WATER_MODEL_GENERATION_RECIPE_VERSION)
        {
            return Err(MapPackageError::InvalidGenerationRecipeVersion);
        }
        let request = request.normalized()?;
        let estimate = request.estimate()?;
        source_locks.sort_by(|left, right| left.id.cmp(&right.id));
        if source_locks.windows(2).any(|pair| pair[0].id == pair[1].id)
            || source_locks.iter().any(|source| {
                source.id.is_empty()
                    || source.provider.is_empty()
                    || source.release.is_empty()
                    || source.url.is_empty()
                    || source.acquired_at.is_empty()
                    || source.native_resolution.is_empty()
                    || source.crs.is_empty()
                    || source.vertical_datum.is_empty()
                    || source.license.is_empty()
                    || source.preprocessing_version.is_empty()
            })
        {
            return Err(MapPackageError::InvalidSourceLocks);
        }
        if projection.horizontal_crs.trim().is_empty() || projection.tool_version.trim().is_empty()
        {
            return Err(MapPackageError::InvalidProjection);
        }
        environment
            .validate()
            .map_err(|_| MapPackageError::InvalidEnvironment)?;
        if let Some(model) = environment
            .hydrology_evidence
            .as_ref()
            .and_then(|index| index.water_model.as_ref())
            && model
                .correction_document
                .validate_for(request, model.samples_per_axis)
                .is_err()
        {
            return Err(MapPackageError::InvalidEnvironment);
        }
        let content_hash = hash_package(
            generator_version,
            request,
            &source_locks,
            &projection,
            &provenance,
            &environment,
            HashMode::Content(Some(generation_recipe_version)),
        );
        Ok(Self {
            schema_version: crate::MAP_SCHEMA_VERSION,
            generator_version,
            generation_recipe_version,
            request,
            estimate,
            source_locks,
            projection,
            provenance,
            environment,
            content_hash,
        })
    }

    pub fn content_hash_hex(&self) -> String {
        self.content_hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Verifies that serialized fields still reproduce the canonical package.
    ///
    /// Callers that load a package from storage must use this before exposing
    /// its chunks. It rejects stale estimates, reordered source locks, and a
    /// content hash that no longer covers the package inputs.
    pub fn validate(&self) -> Result<(), MapPackageError> {
        if self.schema_version != crate::MAP_SCHEMA_VERSION
            && !(self.schema_version == crate::LEGACY_MAP_SCHEMA_VERSION
                && self.environment.hydrology_evidence.is_none())
        {
            return Err(MapPackageError::NonCanonicalFields);
        }
        let mut canonical = Self::with_generation_recipe(
            self.generator_version,
            self.generation_recipe_version,
            self.request,
            self.source_locks.clone(),
            self.projection.clone(),
            self.provenance.clone(),
            self.environment.clone(),
        )?;
        // Schema 9 adds optional identity fields. Keep schema-8 packages whose
        // omitted evidence field defaults to None on their original hash.
        if self.schema_version == crate::LEGACY_MAP_SCHEMA_VERSION {
            canonical.schema_version = crate::LEGACY_MAP_SCHEMA_VERSION;
        }
        (canonical == *self)
            .then_some(())
            .ok_or(MapPackageError::NonCanonicalFields)
    }

    pub fn generator(&self) -> MapChunkGenerator {
        let geography_key = hash_package(
            self.generator_version,
            self.request,
            &self.source_locks,
            &self.projection,
            &self.provenance,
            &self.environment,
            HashMode::Geography,
        );
        MapChunkGenerator::new(
            geography_key,
            self.request.seed,
            self.estimate.tiles_per_side as i32,
        )
        .with_elevation_sampling_recipe(self.generation_recipe_version)
    }

    pub fn chunk_count_per_side(&self) -> u64 {
        self.estimate.tiles_per_side.div_ceil(CHUNK_TILES as u64)
    }
}

fn legacy_generation_recipe_version() -> u16 {
    LEGACY_GENERATION_RECIPE_VERSION
}

fn is_legacy_generation_recipe_version(version: &u16) -> bool {
    *version == LEGACY_GENERATION_RECIPE_VERSION
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MapPackageError {
    #[error(transparent)]
    Request(#[from] MapRequestError),
    #[error("source locks require unique nonempty identifiers")]
    InvalidSourceLocks,
    #[error("projection metadata requires a horizontal CRS")]
    InvalidProjection,
    #[error("prepared environmental index is invalid")]
    InvalidEnvironment,
    #[error("generation recipe version is unsupported")]
    InvalidGenerationRecipeVersion,
    #[error("package fields do not reproduce the canonical package")]
    NonCanonicalFields,
}

#[derive(Clone, Copy)]
enum HashMode {
    Geography,
    Content(Option<u16>),
}

fn hash_package(
    generator_version: u16,
    request: MapRequest,
    source_locks: &[SourceLock],
    projection: &ProjectionMetadata,
    provenance: &EnvironmentalProvenance,
    environment: &PreparedEnvironment,
    mode: HashMode,
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"aoe-map-package-v1\0");
    hash.update(&generator_version.to_le_bytes());
    hash.update(&request.schema_version.to_le_bytes());
    hash.update(&request.center_latitude_e7.to_le_bytes());
    hash.update(&request.center_longitude_e7.to_le_bytes());
    hash.update(&request.requested_side_meters.to_le_bytes());
    hash.update(&request.compression.numerator.to_le_bytes());
    hash.update(&request.compression.denominator.to_le_bytes());
    hash.update(&request.year_ce.to_le_bytes());
    hash.update(&[request.reconstruction_profile as u8]);
    hash.update(&[request.detail_profile as u8]);
    if !matches!(mode, HashMode::Geography) {
        hash.update(&request.seed.to_le_bytes());
    }
    hash_field(&mut hash, projection.horizontal_crs.as_bytes());
    hash.update(&[projection.vertical_datum as u8]);
    hash_field(&mut hash, projection.tool_version.as_bytes());
    hash.update(&[
        provenance.elevation as u8,
        provenance.water as u8,
        provenance.vegetation as u8,
        provenance.historical_land_use as u8,
    ]);
    environment.hash_into(&mut hash);
    if let HashMode::Content(Some(generation_recipe_version)) = mode {
        // Generation behavior is part of the immutable content identity, while
        // the geography key remains stable when only the generation recipe changes.
        hash.update(b"aoe-map-resource-recipe-v1\0");
        hash.update(&generation_recipe_version.to_le_bytes());
    }
    for source in source_locks {
        hash_field(&mut hash, source.id.as_bytes());
        hash_field(&mut hash, source.provider.as_bytes());
        hash_field(&mut hash, source.release.as_bytes());
        hash_field(&mut hash, source.url.as_bytes());
        hash.update(&source.sha256);
        // Acquisition time is provenance, not a prepared input. It must not
        // make identical packages hash differently across cache refreshes.
        hash_field(&mut hash, source.native_resolution.as_bytes());
        hash_field(&mut hash, source.crs.as_bytes());
        hash_field(&mut hash, source.vertical_datum.as_bytes());
        hash_field(&mut hash, source.license.as_bytes());
        hash_field(&mut hash, source.preprocessing_version.as_bytes());
    }
    *hash.finalize().as_bytes()
}

fn hash_field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

#[cfg(test)]
mod tests;
