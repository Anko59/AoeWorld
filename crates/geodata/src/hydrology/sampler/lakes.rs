use super::super::{GeodataError, HydrologyKind, MAX_PAGE_FEATURES, PAGE};
use super::super::hydrology_sampling::VectorFeature;
use gdal::{
    DriverManager,
    raster::{RasterizeOptions, rasterize},
};

const MAX_LAKE_RASTER_CELLS: usize = 64 * 64;

/// Rasterizes one bounded hydrology page at its sample centers. Features are
/// consumed so the geometry buffers are moved into GDAL without cloning.
pub(super) fn rasterize_lakes(
    features: Vec<VectorFeature>,
    width: usize,
    height: usize,
    left: f64,
    top: f64,
    spacing: f64,
) -> Result<Vec<Option<HydrologyKind>>, GeodataError> {
    let Some(cell_count) = width.checked_mul(height) else {
        return Err(GeodataError::Preparation(
            "HydroLAKES raster dimensions overflow",
        ));
    };
    if width == 0
        || height == 0
        || width > usize::from(PAGE)
        || height > usize::from(PAGE)
        || cell_count > MAX_LAKE_RASTER_CELLS
        || features.len() > MAX_PAGE_FEATURES
        || !left.is_finite()
        || !top.is_finite()
        || !spacing.is_finite()
        || spacing <= 0.0
    {
        return Err(GeodataError::Preparation(
            "HydroLAKES raster page is outside its supported bounds",
        ));
    }
    if features.is_empty() {
        return Ok(vec![None; cell_count]);
    }

    let mut geometries = Vec::with_capacity(features.len());
    let mut burns = Vec::with_capacity(features.len());
    // Rasterize overwrites an already-burned cell. Reverse source order so
    // the first matching vector feature retains the same precedence as find.
    for feature in features.into_iter().rev() {
        geometries.push(feature.geometry);
        burns.push(lake_burn_value(feature.kind)?);
    }

    let driver = DriverManager::get_driver_by_name("MEM")?;
    let mut raster = driver.create_with_band_type::<u8, _>("lake-page", width, height, 1)?;
    raster.set_geo_transform(&[left, spacing, 0.0, top, 0.0, -spacing])?;
    raster.rasterband(1)?.fill(0.0, None)?;
    rasterize(
        &mut raster,
        &[1],
        &geometries,
        &burns,
        Some(RasterizeOptions {
            all_touched: false,
            ..RasterizeOptions::default()
        }),
    )?;
    let values = raster
        .rasterband(1)?
        .read_as::<u8>((0, 0), (width, height), (width, height), None)?;
    values
        .data()
        .iter()
        .copied()
        .map(lake_kind_from_burn)
        .collect()
}

fn lake_burn_value(kind: HydrologyKind) -> Result<f64, GeodataError> {
    match kind {
        HydrologyKind::Lake | HydrologyKind::Reservoir | HydrologyKind::RegulatedLake => {
            Ok(f64::from(kind as u8))
        }
        HydrologyKind::UnknownWater => Ok(f64::from(kind as u8)),
        _ => Err(GeodataError::Preparation(
            "HydroLAKES feature has an unsupported kind",
        )),
    }
}

fn lake_kind_from_burn(value: u8) -> Result<Option<HydrologyKind>, GeodataError> {
    match value {
        0 => Ok(None),
        value if value == HydrologyKind::Lake as u8 => Ok(Some(HydrologyKind::Lake)),
        value if value == HydrologyKind::Reservoir as u8 => Ok(Some(HydrologyKind::Reservoir)),
        value if value == HydrologyKind::RegulatedLake as u8 => {
            Ok(Some(HydrologyKind::RegulatedLake))
        }
        value if value == HydrologyKind::UnknownWater as u8 => Ok(Some(HydrologyKind::UnknownWater)),
        _ => Err(GeodataError::Preparation(
            "HydroLAKES raster contains an unknown kind",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdal::vector::Geometry;

    const MULTIPOLYGON: &str = "MULTIPOLYGON (((0 0, 4 0, 4 4, 0 4, 0 0), (1 1, 3 1, 3 3, 1 3, 1 1)), ((5 0, 7 0, 7 2, 5 2, 5 0)))";
    const RESERVOIR_OVERLAP: &str = "POLYGON ((2 0, 6 0, 6 4, 2 4, 2 0))";
    const REGULATED_OVERLAP: &str = "POLYGON ((3 0, 5 0, 5 4, 3 4, 3 0))";

    fn features(definitions: &[(&str, HydrologyKind)]) -> Vec<VectorFeature> {
        definitions
            .iter()
            .map(|(wkt, kind)| VectorFeature {
                geometry: Geometry::from_wkt(wkt).expect("valid test geometry"),
                kind: *kind,
                river_reach: None,
            })
            .collect()
    }

    #[test]
    fn rasterization_preserves_multipolygons_holes_and_first_feature_precedence() {
        let definitions = [
            (MULTIPOLYGON, HydrologyKind::Lake),
            (RESERVOIR_OVERLAP, HydrologyKind::Reservoir),
            (REGULATED_OVERLAP, HydrologyKind::RegulatedLake),
        ];
        let width = 7;
        let height = 6;
        let sampled = rasterize_lakes(features(&definitions), width, height, 0.0, 6.0, 1.0)
            .expect("bounded lake raster");
        let source = features(&definitions);

        for y in 0..height {
            for x in 0..width {
                let point = Geometry::from_wkt(&format!(
                    "POINT ({} {})",
                    x as f64 + 0.5,
                    6.0 - y as f64 - 0.5
                ))
                .expect("sample point");
                // Boundaries have their own regression below. For other
                // centers, the fast raster result must match the old query.
                if source.iter().any(|feature| feature.geometry.touches(&point)) {
                    continue;
                }
                let expected = source
                    .iter()
                    .find(|feature| feature.geometry.contains(&point))
                    .map(|feature| feature.kind);
                assert_eq!(sampled[y * width + x], expected, "cell ({x}, {y})");
            }
        }

        let at = |x: usize, y: usize| sampled[y * width + x];
        assert_eq!(at(0, 2), Some(HydrologyKind::Lake));
        assert_eq!(at(1, 3), None, "the polygon hole remains empty");
        assert_eq!(at(3, 3), Some(HydrologyKind::Lake));
        assert_eq!(at(5, 4), Some(HydrologyKind::Lake), "second polygon burns");
        assert_eq!(at(4, 4), Some(HydrologyKind::Reservoir));
    }

    #[test]
    fn center_on_a_polygon_boundary_is_not_expanded_as_all_touched() {
        let polygon = "POLYGON ((0.5 0.5, 2.5 0.5, 2.5 3.5, 0.5 3.5, 0.5 0.5))";
        let sampled = rasterize_lakes(
            features(&[(polygon, HydrologyKind::Lake)]),
            4,
            4,
            0.0,
            4.0,
            1.0,
        )
        .expect("bounded lake raster");
        assert_eq!(sampled[1], None, "left-edge centers are excluded");
        assert_eq!(sampled[3], None, "right-edge centers are excluded");
        assert_eq!(sampled[4], None, "outside cells are not expanded");
        assert_eq!(sampled[5], Some(HydrologyKind::Lake));
    }

    #[test]
    fn lake_raster_rejects_dimensions_above_one_page_before_allocation() {
        assert!(rasterize_lakes(Vec::new(), 0, 1, 0.0, 1.0, 1.0).is_err());
        assert!(rasterize_lakes(Vec::new(), 65, 1, 0.0, 1.0, 1.0).is_err());
        assert!(rasterize_lakes(Vec::new(), usize::MAX, 2, 0.0, 1.0, 1.0).is_err());
    }
}
