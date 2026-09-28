use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, local_aeqd_definition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, MapRequest, PotentialBiomePage, PyramidLevel,
    ordered_biome_page_root,
};
use gdal::{
    Dataset, GeoTransformEx,
    raster::ResampleAlg,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::{fs, path::Path};

mod correction;
pub use correction::{
    VEGETATION_PATCH_PREPROCESSING_IDENTITY, VegetationPatch, VegetationPatchBinding,
    VegetationPatchDocument, VegetationPatchOperation, VegetationPatchSource,
};

#[derive(Clone, Debug)]
pub struct PreparedVegetation {
    pub field: FieldPyramid,
    pub pages: Vec<PotentialBiomePage>,
}

/// Rejects a changed class legend before raster values reach the immutable map
/// contract. The game mapping intentionally covers the source's consolidated
/// natural-biome categories, so class-number drift would be a data change.
pub fn verify_potential_biome_legend(path: &Path) -> Result<(), GeodataError> {
    let legend = fs::read_to_string(path)?;
    legend_is_compatible(&legend)
        .then_some(())
        .ok_or(GeodataError::Preparation(
            "potential biome class legend is incompatible",
        ))
}

fn legend_is_compatible(legend: &str) -> bool {
    let expected = [
        "tropical evergreen broadleaf forest",
        "cool evergreen needleleaf forest",
        "temperate deciduous broadleaf forest",
        "tropical savanna",
        "steppe",
        "desert",
        "graminoid and forb tundra",
    ];
    legend.starts_with("\"\",\"Number\",\"New.global.consolidated.biome.scheme\"")
        && expected.iter().all(|name| legend.contains(name))
}

/// Reprojects the published potential-natural-vegetation classes with nearest
/// sampling. Class zero is reserved for source nodata and is retained so the
/// map core can fall back explicitly rather than inventing a source class.
pub fn prepare_potential_biomes(
    path: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedVegetation, GeodataError> {
    let corrections = VegetationPatchDocument::empty(request, samples_per_axis)?;
    prepare_potential_biomes_with_corrections(path, request, samples_per_axis, &corrections)
}

pub fn prepare_potential_biomes_with_corrections(
    path: &Path,
    request: MapRequest,
    samples_per_axis: u16,
    corrections: &VegetationPatchDocument,
) -> Result<PreparedVegetation, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "working vegetation grid is outside direct bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    corrections.validate_for(request, samples_per_axis)?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    let dataset = Dataset::open(path)?;
    let definition = local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let mut target =
        SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
    let mut source = dataset
        .spatial_ref()
        .map_err(|_| GeodataError::Projection)?;
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&target, &source).map_err(|_| GeodataError::Projection)?;
    let (width, height) = dataset.raster_size();
    let inverse = dataset.geo_transform()?.invert()?;
    let side = estimate.effective_side_meters as f64;
    let spacing = side / f64::from(samples_per_axis);
    let mut east = Vec::with_capacity(usize::from(samples_per_axis).pow(2));
    let mut north = Vec::with_capacity(east.capacity());
    for y in 0..samples_per_axis {
        for x in 0..samples_per_axis {
            east.push(-side / 2.0 + (f64::from(x) + 0.5) * spacing);
            north.push(side / 2.0 - (f64::from(y) + 0.5) * spacing);
        }
    }
    transform
        .transform_coords(&mut east, &mut north, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    let band = dataset.rasterband(1)?;
    let nodata = band.no_data_value();
    let mut classes = Vec::with_capacity(east.len());
    for (source_x, source_y) in east.into_iter().zip(north) {
        let (pixel, line) = inverse.apply(source_x, source_y);
        let pixel = pixel.floor() as isize;
        let line = line.floor() as isize;
        if pixel < 0 || line < 0 || pixel >= width as isize || line >= height as isize {
            classes.push(0);
            continue;
        }
        let value = band
            .read_as::<f64>(
                (pixel, line),
                (1, 1),
                (1, 1),
                Some(ResampleAlg::NearestNeighbour),
            )?
            .data()[0];
        if !value.is_finite()
            || nodata.is_some_and(|missing| value == missing)
            || !(0.0..=u8::MAX as f64).contains(&value)
        {
            classes.push(0);
        } else {
            classes.push(value.round() as u8);
        }
    }
    corrections.apply(samples_per_axis, &mut classes)?;
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut axis = samples_per_axis;
    let mut values = classes;
    loop {
        let level_pages = pages_for(levels.len() as u8, axis, &values)?;
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_biome_page_root(&level_pages)?,
        });
        pages.extend(level_pages);
        if axis == 1 {
            break;
        }
        values = reduce_classes(axis, &values)?;
        axis = axis.div_ceil(2);
    }
    Ok(PreparedVegetation {
        field: FieldPyramid { levels },
        pages,
    })
}

fn pages_for(level: u8, axis: u16, values: &[u8]) -> Result<Vec<PotentialBiomePage>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation(
            "vegetation grid shape is invalid",
        ));
    }
    let mut pages = Vec::new();
    for y in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
        for x in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
            let width = (axis - x).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let height = (axis - y).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let mut classes = Vec::with_capacity(usize::from(width) * usize::from(height));
            for row in y..y + height {
                let start = usize::from(row) * usize::from(axis) + usize::from(x);
                classes.extend_from_slice(&values[start..start + usize::from(width)]);
            }
            pages.push(PotentialBiomePage {
                level,
                x: x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                y: y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                width: width as u8,
                height: height as u8,
                potential_biome_class: classes,
            });
        }
    }
    Ok(pages)
}

fn reduce_classes(axis: u16, values: &[u8]) -> Result<Vec<u8>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation(
            "vegetation grid shape is invalid",
        ));
    }
    let next_axis = axis.div_ceil(2);
    let mut reduced = Vec::with_capacity(usize::from(next_axis).pow(2));
    for y in 0..next_axis {
        for x in 0..next_axis {
            let mut candidates = [0_u8; 4];
            let mut len = 0;
            for source_y in y * 2..((y + 1) * 2).min(axis) {
                for source_x in x * 2..((x + 1) * 2).min(axis) {
                    candidates[len] =
                        values[usize::from(source_y) * usize::from(axis) + usize::from(source_x)];
                    len += 1;
                }
            }
            candidates[..len].sort_unstable();
            let mut best = candidates[0];
            let mut best_count = 0;
            for candidate in &candidates[..len] {
                let count = candidates[..len]
                    .iter()
                    .filter(|value| *value == candidate)
                    .count();
                if count > best_count {
                    best = *candidate;
                    best_count = count;
                }
            }
            reduced.push(best);
        }
    }
    Ok(reduced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdal::{DriverManager, raster::Buffer, spatial_ref::SpatialRef};

    #[test]
    fn categorical_reduction_uses_the_stable_lowest_tie_breaker() {
        assert_eq!(reduce_classes(2, &[3, 1, 3, 1]).expect("reduced"), [1]);
    }

    #[test]
    fn legend_requires_the_source_header_and_representative_classes() {
        let legend = "\"\",\"Number\",\"New.global.consolidated.biome.scheme\"\n\
             tropical evergreen broadleaf forest\n\
             cool evergreen needleleaf forest\n\
             temperate deciduous broadleaf forest\n\
             tropical savanna\nsteppe\ndesert\ngraminoid and forb tundra\n";
        assert!(legend_is_compatible(legend));
        assert!(!legend_is_compatible("incompatible"));
    }

    #[test]
    fn native_raster_sampling_preserves_valid_and_invalid_classes() {
        let root = std::env::temp_dir().join(format!(
            "aoe-vegetation-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("temporary directory");
        let valid = root.join("valid.tif");
        write_raster(&valid, 7.0, None);
        let prepared = prepare_potential_biomes(&valid, MapRequest::default(), 2)
            .expect("valid vegetation raster");
        assert_eq!(prepared.pages.len(), 2);
        assert!(
            prepared.pages[0]
                .potential_biome_class
                .iter()
                .all(|class| *class == 7)
        );

        let mut correction = VegetationPatchDocument::empty(MapRequest::default(), 2).unwrap();
        correction.sources.push(VegetationPatchSource {
            id: "fixture".into(),
            citation: "Offline historical biome fixture".into(),
        });
        correction.patches.push(VegetationPatch {
            id: "override".into(),
            source_id: "fixture".into(),
            priority: 0,
            rectangle_east_north_meters: [-10_000, -10_000, 10_000, 10_000],
            applicable_year_start_ce: 500,
            applicable_year_end_ce: 700,
            operation: VegetationPatchOperation::HistoricalBiome { class: 27 },
        });
        let corrected = prepare_potential_biomes_with_corrections(
            &valid,
            MapRequest::default(),
            2,
            &correction,
        )
        .expect("corrected vegetation raster");
        assert_eq!(corrected.pages[0].potential_biome_class, [27; 4]);
        assert_ne!(
            corrected.field.levels[0].ordered_page_root,
            prepared.field.levels[0].ordered_page_root
        );

        let invalid = root.join("invalid.tif");
        write_raster(&invalid, 999.0, None);
        let prepared = prepare_potential_biomes(&invalid, MapRequest::default(), 2)
            .expect("out-of-range vegetation classes become nodata");
        assert!(
            prepared.pages[0]
                .potential_biome_class
                .iter()
                .all(|class| *class == 0)
        );

        let nodata = root.join("nodata.tif");
        write_raster(&nodata, -1.0, Some(-1.0));
        let prepared = prepare_potential_biomes(&nodata, MapRequest::default(), 2)
            .expect("vegetation nodata becomes class zero");
        assert!(
            prepared.pages[0]
                .potential_biome_class
                .iter()
                .all(|class| *class == 0)
        );
        assert!(matches!(
            prepare_potential_biomes(&valid, MapRequest::default(), 1),
            Err(GeodataError::Preparation(_))
        ));
        fs::remove_dir_all(root).expect("remove temporary directory");
    }

    fn write_raster(path: &Path, value: f64, nodata: Option<f64>) {
        let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
        let mut dataset = driver
            .create_with_band_type::<f64, _>(path, 32, 32, 1)
            .expect("vegetation raster");
        dataset
            .set_geo_transform(&[1.0, 0.1, 0.0, 50.5, 0.0, -0.1])
            .expect("geotransform");
        dataset
            .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
            .expect("spatial reference");
        let mut band = dataset.rasterband(1).expect("vegetation band");
        if let Some(nodata) = nodata {
            band.set_no_data_value(Some(nodata)).expect("nodata");
        }
        let mut values = Buffer::new((32, 32), vec![value; 32 * 32]);
        band.write((0, 0), (32, 32), &mut values)
            .expect("vegetation values");
        dataset.flush_cache().expect("flush raster");
    }
}
