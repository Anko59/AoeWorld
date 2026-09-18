use crate::{CHUNK_TILES, MapChunkGenerator, MapEstimate, MapRequest, MapRequestError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionMetadata {
    pub horizontal_crs: String,
    pub vertical_datum: VerticalDatum,
}

impl Default for ProjectionMetadata {
    fn default() -> Self {
        Self {
            horizontal_crs: "fallback-local-grid-v1".to_owned(),
            vertical_datum: VerticalDatum::UnspecifiedFallback,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalDatum {
    Egm2008Orthometric,
    UnspecifiedFallback,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceLock {
    pub id: String,
    pub release: String,
    pub url: String,
    pub sha256: [u8; 32],
    pub native_resolution_millimeters: u64,
    pub crs: String,
    pub license: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapPackage {
    pub schema_version: u16,
    pub generator_version: u16,
    pub request: MapRequest,
    pub estimate: MapEstimate,
    pub source_locks: Vec<SourceLock>,
    pub projection: ProjectionMetadata,
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
        mut source_locks: Vec<SourceLock>,
        projection: ProjectionMetadata,
    ) -> Result<Self, MapPackageError> {
        let request = request.normalized()?;
        let estimate = request.estimate()?;
        source_locks.sort_by(|left, right| left.id.cmp(&right.id));
        if source_locks.windows(2).any(|pair| pair[0].id == pair[1].id)
            || source_locks.iter().any(|source| source.id.is_empty())
        {
            return Err(MapPackageError::InvalidSourceLocks);
        }
        if projection.horizontal_crs.trim().is_empty() {
            return Err(MapPackageError::InvalidProjection);
        }
        let content_hash =
            hash_package(generator_version, request, &source_locks, &projection, true);
        Ok(Self {
            schema_version: 1,
            generator_version,
            request,
            estimate,
            source_locks,
            projection,
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
        let canonical = Self::with_projection(
            self.generator_version,
            self.request,
            self.source_locks.clone(),
            self.projection.clone(),
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
    #[error("package fields do not reproduce the canonical package")]
    NonCanonicalFields,
}

fn hash_package(
    generator_version: u16,
    request: MapRequest,
    source_locks: &[SourceLock],
    projection: &ProjectionMetadata,
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
    for source in source_locks {
        hash_field(&mut hash, source.id.as_bytes());
        hash_field(&mut hash, source.release.as_bytes());
        hash_field(&mut hash, source.url.as_bytes());
        hash.update(&source.sha256);
        hash.update(&source.native_resolution_millimeters.to_le_bytes());
        hash_field(&mut hash, source.crs.as_bytes());
        hash_field(&mut hash, source.license.as_bytes());
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

    fn source(id: &str) -> SourceLock {
        SourceLock {
            id: id.to_owned(),
            release: "test".to_owned(),
            url: "https://example.invalid/test".to_owned(),
            sha256: [7; 32],
            native_resolution_millimeters: 30_000,
            crs: "EPSG:4326".to_owned(),
            license: "test-only".to_owned(),
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
            },
        )
        .expect("package");
        assert_ne!(first.content_hash, second.content_hash);
    }
}
