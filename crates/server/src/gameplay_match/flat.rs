//! A flat synthetic ground for AI development: one elevation everywhere, so
//! only trees, bushes and ore obstruct movement. Generated terrain without
//! source data is rugged (cliffs wall off most villages), which makes it a
//! poor place to teach an AI to build and fight.
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage,
    MapPackageError, MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, Ratio,
    ordered_page_root,
};
use serde::Deserialize;

/// Tiles per side bounds of a flat map.
const MIN_TILES: u32 = 64;
const MAX_TILES: u32 = 4_096;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FlatMap {
    pub tiles_per_side: u32,
    #[serde(default = "one")]
    pub seed: u64,
}

fn one() -> u64 {
    1
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FlatError {
    #[error("a flat map has {MIN_TILES} to {MAX_TILES} tiles per side")]
    Size,
    #[error(transparent)]
    Package(#[from] MapPackageError),
}

/// The package (2 m tiles at 1:1, so `tiles_per_side * 2` metres) and its
/// elevation pages.
pub(crate) fn package(flat: FlatMap) -> Result<(MapPackage, Vec<ElevationPage>), FlatError> {
    if !(MIN_TILES..=MAX_TILES).contains(&flat.tiles_per_side) {
        return Err(FlatError::Size);
    }
    let page = |level, samples: u8| ElevationPage {
        level,
        x: 0,
        y: 0,
        width: samples,
        height: samples,
        geographic_height_centimeters: vec![0; usize::from(samples).pow(2)],
    };
    let (ground, overview) = (page(0, 2), page(1, 1));
    let level = |samples_per_axis, page: &ElevationPage| -> Result<PyramidLevel, FlatError> {
        Ok(PyramidLevel {
            samples_per_axis,
            ordered_page_root: ordered_page_root(std::slice::from_ref(page))
                .map_err(|_| FlatError::Size)?,
        })
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![level(2, &ground)?, level(1, &overview)?],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let request = MapRequest {
        requested_side_meters: u64::from(flat.tiles_per_side) * 2,
        compression: Ratio {
            numerator: 1,
            denominator: 1,
        },
        seed: flat.seed,
        ..MapRequest::default()
    };
    let package = MapPackage::with_prepared_environment(
        1,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )?;
    Ok((package, vec![ground, overview]))
}
