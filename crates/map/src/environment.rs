use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_ENVIRONMENT_SAMPLES_PER_AXIS: u16 = 16_384;
pub const ENVIRONMENT_PAGE_SAMPLES: u8 = 64;
const MAX_PYRAMID_LEVELS: usize = 15;

/// Immutable, externalized environmental field index. Page bytes live in the
/// server adapter; their ordered BLAKE3 roots are part of map identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PreparedEnvironment {
    pub samples_per_axis: u16,
    pub geographic_millimeters_per_sample: u64,
    pub page_samples: u8,
    pub elevation: FieldPyramid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FieldPyramid {
    pub levels: Vec<PyramidLevel>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PyramidLevel {
    pub samples_per_axis: u16,
    pub ordered_page_root: [u8; 32],
}

/// One bounded environmental page, stored and verified independently from the
/// package index. Heights use the selected source vertical datum in centimeters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElevationPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub geographic_height_centimeters: Vec<i32>,
}

impl PreparedEnvironment {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.samples_per_axis == 0 {
            return self
                .elevation
                .levels
                .is_empty()
                .then_some(())
                .ok_or(EnvironmentError::InvalidPyramid);
        }
        if self.samples_per_axis > MAX_ENVIRONMENT_SAMPLES_PER_AXIS
            || self.geographic_millimeters_per_sample == 0
            || self.page_samples != ENVIRONMENT_PAGE_SAMPLES
        {
            return Err(EnvironmentError::InvalidIndex);
        }
        self.elevation.validate(self.samples_per_axis)
    }

    pub(crate) fn hash_into(&self, hash: &mut blake3::Hasher) {
        hash.update(&self.samples_per_axis.to_le_bytes());
        hash.update(&self.geographic_millimeters_per_sample.to_le_bytes());
        hash.update(&[self.page_samples]);
        hash.update(&(self.elevation.levels.len() as u64).to_le_bytes());
        for level in &self.elevation.levels {
            hash.update(&level.samples_per_axis.to_le_bytes());
            hash.update(&level.ordered_page_root);
        }
    }
}

impl FieldPyramid {
    fn validate(&self, samples_per_axis: u16) -> Result<(), EnvironmentError> {
        if self.levels.is_empty() || self.levels.len() > MAX_PYRAMID_LEVELS {
            return Err(EnvironmentError::InvalidPyramid);
        }
        let mut expected = samples_per_axis;
        for level in &self.levels {
            if level.samples_per_axis != expected || level.ordered_page_root == [0; 32] {
                return Err(EnvironmentError::InvalidPyramid);
            }
            expected = expected.div_ceil(2);
        }
        (expected == 1)
            .then_some(())
            .ok_or(EnvironmentError::InvalidPyramid)
    }
}

impl ElevationPage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.geographic_height_centimeters.len()
                != usize::from(self.width) * usize::from(self.height)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-elevation-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        for height in &self.geographic_height_centimeters {
            hash.update(&height.to_le_bytes());
        }
        Ok(*hash.finalize().as_bytes())
    }
}

pub fn ordered_page_root(pages: &[ElevationPage]) -> Result<[u8; 32], EnvironmentError> {
    if pages.is_empty() {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut ordered = pages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|page| (page.y, page.x));
    if ordered
        .windows(2)
        .any(|pair| (pair[0].x, pair[0].y) == (pair[1].x, pair[1].y))
    {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut hash = blake3::Hasher::new();
    hash.update(b"aoe-environment-page-root-v1\0");
    hash.update(&(ordered.len() as u64).to_le_bytes());
    for page in ordered {
        hash.update(&page.content_hash()?);
    }
    Ok(*hash.finalize().as_bytes())
}

pub(crate) fn level_zero_pages(
    environment: &PreparedEnvironment,
    pages: Vec<ElevationPage>,
) -> Result<BTreeMap<(u16, u16), ElevationPage>, EnvironmentError> {
    environment.validate()?;
    let mut levels = (0..environment.elevation.levels.len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<ElevationPage>>>();
    for page in pages {
        let level = usize::from(page.level);
        let metadata = environment
            .elevation
            .levels
            .get(level)
            .ok_or(EnvironmentError::InvalidPyramid)?;
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        page.validate()?;
        if page.x >= count || page.y >= count {
            return Err(EnvironmentError::InvalidPyramid);
        }
        levels[level].push(page);
    }
    let mut level_zero = BTreeMap::new();
    for (level, (metadata, level_pages)) in
        environment.elevation.levels.iter().zip(levels).enumerate()
    {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_page_root(&level_pages)? != metadata.ordered_page_root
        {
            return Err(EnvironmentError::InvalidPyramid);
        }
        if level == 0 {
            for page in level_pages {
                if level_zero.insert((page.x, page.y), page).is_some() {
                    return Err(EnvironmentError::InvalidPyramid);
                }
            }
        }
    }
    Ok(level_zero)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EnvironmentError {
    #[error("environmental field index is invalid")]
    InvalidIndex,
    #[error("environmental field pyramid is invalid")]
    InvalidPyramid,
    #[error("environmental page is invalid")]
    InvalidPage,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> ElevationPage {
        ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![1, 2, 3, 4],
        }
    }

    #[test]
    fn elevation_pages_are_bounded_and_content_addressed() {
        let original = page().content_hash().expect("original hash");
        let mut changed = page();
        changed.geographic_height_centimeters[3] = 5;
        assert_ne!(original, changed.content_hash().expect("changed hash"));
        changed.geographic_height_centimeters.pop();
        assert_eq!(changed.content_hash(), Err(EnvironmentError::InvalidPage));
    }

    #[test]
    fn page_roots_are_canonical_and_reject_duplicate_locations() {
        let first = page();
        let mut second = page();
        second.x = 1;
        assert_eq!(
            ordered_page_root(&[first.clone(), second.clone()]),
            ordered_page_root(&[second, first.clone()])
        );
        assert_eq!(
            ordered_page_root(&[first.clone(), first]),
            Err(EnvironmentError::InvalidPyramid)
        );
    }

    #[test]
    fn prepared_field_requires_a_complete_bounded_pyramid() {
        let field = PreparedEnvironment {
            samples_per_axis: 4,
            geographic_millimeters_per_sample: 30_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![
                    PyramidLevel {
                        samples_per_axis: 4,
                        ordered_page_root: [1; 32],
                    },
                    PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: [2; 32],
                    },
                    PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: [3; 32],
                    },
                ],
            },
        };
        assert!(field.validate().is_ok());
        let mut invalid = field;
        invalid.elevation.levels[1].samples_per_axis = 3;
        assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidPyramid));
    }

    #[test]
    fn complete_prepared_pages_override_fallback_relief_and_apply_compression() {
        let first = ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![-100, 200, 300, 400],
        };
        let second = ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![250],
        };
        let environment = PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 125_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![
                    PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&first))
                            .expect("root"),
                    },
                    PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&second))
                            .expect("root"),
                    },
                ],
            },
        };
        let request = crate::MapRequest {
            requested_side_meters: 250,
            compression: crate::Ratio::new(1, 1).expect("compression"),
            ..crate::MapRequest::default()
        };
        let package = crate::MapPackage::with_prepared_environment(
            1,
            request,
            Vec::new(),
            crate::ProjectionMetadata::default(),
            crate::EnvironmentalProvenance::default(),
            environment,
        )
        .expect("package");
        let terrain = package
            .generator_with_elevation(vec![first, second])
            .expect("prepared terrain");
        let low_elevation = terrain
            .tile_at(aoe_core::TileCoord::new(0, 0))
            .expect("low elevation");
        assert_eq!(low_elevation.geographic_height_centimeters, -100);
        assert_eq!(
            low_elevation.elevation_provenance,
            crate::Provenance::SourceDerived
        );
        assert_eq!(low_elevation.water_provenance, crate::Provenance::Fallback);
        let tile = terrain
            .tile_at(aoe_core::TileCoord::new(124, 124))
            .expect("tile");
        assert_eq!(tile.geographic_height_centimeters, 400);
        assert_eq!(tile.game_height_level, 4);
        assert_eq!(tile.elevation_provenance, crate::Provenance::SourceDerived);
    }
}
