use crate::{GeodataError, local_aeqd_definition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, MapRequest, PreparedEnvironment,
    PyramidLevel, ordered_page_root,
};
use gdal::{
    Dataset, GeoTransformEx,
    raster::ResampleAlg,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::path::Path;

/// Direct preparation remains intentionally bounded until page streaming is
/// introduced. Larger requests must use the streaming worker path.
pub const MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS: u16 = 128;

#[derive(Clone, Debug)]
pub struct PreparedElevation {
    pub environment: PreparedEnvironment,
    pub pages: Vec<ElevationPage>,
}

pub fn prepare_elevation(
    path: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedElevation, GeodataError> {
    let dataset = Dataset::open(path)?;
    prepare_elevation_dataset(&dataset, request, samples_per_axis)
}

fn prepare_elevation_dataset(
    dataset: &Dataset,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedElevation, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "working grid is outside direct bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
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
    let mut heights = Vec::with_capacity(east.len());
    for (source_x, source_y) in east.into_iter().zip(north) {
        let (pixel, line) = inverse.apply(source_x, source_y);
        let pixel = pixel.floor() as isize;
        let line = line.floor() as isize;
        if pixel < 0 || line < 0 || pixel >= width as isize || line >= height as isize {
            return Err(GeodataError::Preparation(
                "source does not cover the requested footprint",
            ));
        }
        let value = band
            .read_as::<f64>(
                (pixel, line),
                (1, 1),
                (1, 1),
                Some(ResampleAlg::NearestNeighbour),
            )?
            .data()[0];
        if !value.is_finite() || nodata.is_some_and(|missing| value == missing) {
            return Err(GeodataError::Preparation(
                "source contains elevation nodata",
            ));
        }
        let centimeters = (value * 100.0).round();
        if centimeters < i32::MIN as f64 || centimeters > i32::MAX as f64 {
            return Err(GeodataError::Preparation(
                "source elevation is outside centimeter bounds",
            ));
        }
        heights.push(centimeters as i32);
    }
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut axis = samples_per_axis;
    let mut values = heights;
    loop {
        let level_pages = pages_for(levels.len() as u8, axis, &values)?;
        let root = ordered_page_root(&level_pages)?;
        pages.extend(level_pages);
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: root,
        });
        if axis == 1 {
            break;
        }
        values = reduce_elevation(axis, &values)?;
        axis = axis.div_ceil(2);
    }
    let geographic_millimeters_per_sample = estimate
        .effective_side_meters
        .checked_mul(1_000)
        .ok_or(GeodataError::Preparation("sample spacing overflows"))?
        .div_ceil(u64::from(samples_per_axis));
    let environment = PreparedEnvironment {
        samples_per_axis,
        geographic_millimeters_per_sample,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid { levels },
        water: None,
    };
    environment.validate()?;
    Ok(PreparedElevation { environment, pages })
}

fn pages_for(level: u8, axis: u16, values: &[i32]) -> Result<Vec<ElevationPage>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation("elevation grid shape is invalid"));
    }
    let mut pages = Vec::new();
    for y in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
        for x in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
            let width = (axis - x).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let height = (axis - y).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let mut page_values = Vec::with_capacity(usize::from(width) * usize::from(height));
            for row in y..y + height {
                let start = usize::from(row) * usize::from(axis) + usize::from(x);
                page_values.extend_from_slice(&values[start..start + usize::from(width)]);
            }
            pages.push(ElevationPage {
                level,
                x: x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                y: y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                width: width as u8,
                height: height as u8,
                geographic_height_centimeters: page_values,
            });
        }
    }
    Ok(pages)
}

fn reduce_elevation(axis: u16, values: &[i32]) -> Result<Vec<i32>, GeodataError> {
    let next_axis = axis.div_ceil(2);
    let mut reduced = Vec::with_capacity(usize::from(next_axis).pow(2));
    for y in 0..next_axis {
        for x in 0..next_axis {
            let mut total = 0_i64;
            let mut count = 0_i64;
            for source_y in y * 2..((y + 1) * 2).min(axis) {
                for source_x in x * 2..((x + 1) * 2).min(axis) {
                    total += i64::from(
                        values[usize::from(source_y) * usize::from(axis) + usize::from(source_x)],
                    );
                    count += 1;
                }
            }
            let rounded = if total.is_negative() {
                (total - count / 2) / count
            } else {
                (total + count / 2) / count
            };
            reduced.push(rounded as i32);
        }
    }
    Ok(reduced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdal::{DriverManager, raster::Buffer};

    #[test]
    fn native_preparation_reprojects_a_bounded_raster_into_complete_pages() {
        let driver = DriverManager::get_driver_by_name("MEM").expect("MEM driver");
        let mut dataset = driver
            .create_with_band_type::<f64, _>("elevation", 4, 4, 1)
            .expect("raster");
        dataset
            .set_geo_transform(&[-2.0, 1.0, 0.0, 2.0, 0.0, -1.0])
            .expect("geotransform");
        dataset
            .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
            .expect("spatial reference");
        let mut values = Buffer::new((4, 4), (0..16).map(f64::from).collect());
        dataset
            .rasterband(1)
            .expect("band")
            .write((0, 0), (4, 4), &mut values)
            .expect("values");
        let prepared = prepare_elevation_dataset(
            &dataset,
            MapRequest {
                center_latitude_e7: 0,
                center_longitude_e7: 0,
                requested_side_meters: 250,
                compression: aoe_map::Ratio::new(1, 1).expect("ratio"),
                ..MapRequest::default()
            },
            4,
        )
        .expect("prepared elevation");
        assert_eq!(prepared.environment.samples_per_axis, 4);
        assert_eq!(prepared.environment.elevation.levels.len(), 3);
        assert_eq!(prepared.pages.len(), 3);
        assert!(prepared.pages.iter().all(|page| page.validate().is_ok()));
    }
}
