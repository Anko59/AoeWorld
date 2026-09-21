use crate::{
    CHUNK_TILES, MapChunkGenerator, MapEstimate, MapRequest, MapRequestError, PreparedEnvironment,
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
        mut source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
        provenance: EnvironmentalProvenance,
        environment: PreparedEnvironment,
    ) -> Result<Self, MapPackageError> {
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
        let content_hash = hash_package(
            generator_version,
            request,
            &source_locks,
            &projection,
            &provenance,
            &environment,
            true,
        );
        Ok(Self {
            schema_version: crate::MAP_SCHEMA_VERSION,
            generator_version,
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
        let canonical = Self::with_prepared_environment(
            self.generator_version,
            self.request,
            self.source_locks.clone(),
            self.projection.clone(),
            self.provenance.clone(),
            self.environment.clone(),
        )?;
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
            false,
        );
        MapChunkGenerator::new(
            geography_key,
            self.request.seed,
            self.estimate.tiles_per_side as i32,
        )
    }

    pub fn chunk_count_per_side(&self) -> u64 {
        self.estimate.tiles_per_side.div_ceil(CHUNK_TILES as u64)
    }
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
    #[error("package fields do not reproduce the canonical package")]
    NonCanonicalFields,
}

fn hash_package(
    generator_version: u16,
    request: MapRequest,
    source_locks: &[SourceLock],
    projection: &ProjectionMetadata,
    provenance: &EnvironmentalProvenance,
    environment: &PreparedEnvironment,
    include_seed: bool,
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
    if include_seed {
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
mod tests {
    use super::*;

    fn environment() -> PreparedEnvironment {
        PreparedEnvironment {
            samples_per_axis: 4,
            geographic_millimeters_per_sample: 30_000,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            elevation: crate::FieldPyramid {
                levels: vec![
                    crate::PyramidLevel {
                        samples_per_axis: 4,
                        ordered_page_root: [1; 32],
                    },
                    crate::PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: [2; 32],
                    },
                    crate::PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: [3; 32],
                    },
                ],
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
        }
    }

    fn source(id: &str) -> SourceLock {
        SourceLock {
            id: id.to_owned(),
            provider: "fixture".to_owned(),
            release: "test".to_owned(),
            url: "https://example.invalid/test".to_owned(),
            sha256: [7; 32],
            acquired_at: "2026-09-18T00:00:00Z".to_owned(),
            native_resolution: "30 meters".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "EGM2008".to_owned(),
            license: "test-only".to_owned(),
            preprocessing_version: "test-v1".to_owned(),
        }
    }

    #[test]
    fn packages_have_canonical_source_order_and_stable_identity() {
        let first = MapPackage::new(1, MapRequest::default(), vec![source("b"), source("a")])
            .expect("package");
        let second = MapPackage::new(1, MapRequest::default(), vec![source("a"), source("b")])
            .expect("package");
        assert_eq!(first, second);
        assert_eq!(first.chunk_count_per_side(), 16);
    }

    #[test]
    fn seed_changes_detail_identity_but_not_geographic_elevation() {
        let first = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
        let second = MapPackage::new(
            1,
            MapRequest {
                seed: 2,
                ..MapRequest::default()
            },
            vec![],
        )
        .expect("package");
        assert_ne!(first.content_hash, second.content_hash);
        assert_eq!(
            first.generator().chunk(0, 0).tiles[0].geographic_height_centimeters,
            second.generator().chunk(0, 0).tiles[0].geographic_height_centimeters
        );
    }

    #[test]
    fn duplicate_source_locks_are_rejected() {
        assert!(matches!(
            MapPackage::new(
                1,
                MapRequest::default(),
                vec![source("same"), source("same")]
            ),
            Err(MapPackageError::InvalidSourceLocks)
        ));
    }

    #[test]
    fn validation_rejects_a_tampered_serialized_field() {
        let mut package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
        package.estimate.tiles_per_side += 1;
        assert_eq!(package.validate(), Err(MapPackageError::NonCanonicalFields));
    }

    #[test]
    fn packages_require_and_hash_projection_metadata() {
        assert!(matches!(
            MapPackage::with_projection(
                1,
                MapRequest::default(),
                Vec::new(),
                ProjectionMetadata {
                    horizontal_crs: " ".to_owned(),
                    vertical_datum: VerticalDatum::UnspecifiedFallback,
                    tool_version: "test".to_owned(),
                },
            ),
            Err(MapPackageError::InvalidProjection)
        ));
        let first = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
        let second = MapPackage::with_projection(
            1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata {
                horizontal_crs: "EPSG:3857".to_owned(),
                vertical_datum: VerticalDatum::Egm2008Orthometric,
                tool_version: "GDAL 3.6.2 / PROJ 9.1.1".to_owned(),
            },
        )
        .expect("package");
        assert_ne!(first.content_hash, second.content_hash);
    }

    #[test]
    fn packages_hash_environmental_provenance() {
        let fallback = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
        let sourced = MapPackage::with_environment(
            1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance {
                elevation: LayerProvenance::SourceDerived,
                water: LayerProvenance::HistoricallyCorrected,
                vegetation: LayerProvenance::ModelDerived,
                historical_land_use: LayerProvenance::SourceDerived,
            },
        )
        .expect("package");
        assert_ne!(fallback.content_hash, sourced.content_hash);
    }

    #[test]
    fn packages_hash_prepared_environment_roots() {
        let package = MapPackage::with_prepared_environment(
            1,
            MapRequest::default(),
            vec![source("elevation")],
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            environment(),
        )
        .expect("prepared package");
        let mut changed_environment = environment();
        changed_environment.elevation.levels[0].ordered_page_root = [4; 32];
        let changed = MapPackage::with_prepared_environment(
            1,
            MapRequest::default(),
            vec![source("elevation")],
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            changed_environment,
        )
        .expect("changed package");
        assert_ne!(package.content_hash, changed.content_hash);
        assert!(package.validate().is_ok());
    }

    #[test]
    fn acquisition_time_is_not_a_content_input_but_preprocessing_is() {
        let first =
            MapPackage::new(1, MapRequest::default(), vec![source("elevation")]).expect("package");
        let mut later_source = source("elevation");
        later_source.acquired_at = "2026-09-19T00:00:00Z".to_owned();
        let later = MapPackage::new(1, MapRequest::default(), vec![later_source]).expect("package");
        assert_eq!(first.content_hash, later.content_hash);
        let mut altered_source = source("elevation");
        altered_source.preprocessing_version = "test-v2".to_owned();
        let altered =
            MapPackage::new(1, MapRequest::default(), vec![altered_source]).expect("package");
        assert_ne!(first.content_hash, altered.content_hash);
    }
}
