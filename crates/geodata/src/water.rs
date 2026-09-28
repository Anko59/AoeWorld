use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, local_aeqd_definition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, MapRequest, PyramidLevel, WaterPage,
    ordered_water_page_root,
};
use gdal::{
    Dataset, DatasetOptions, DriverManager, GdalOpenFlags,
    raster::rasterize,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
    vector::LayerAccess,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const COASTLINE_SUPERSAMPLE: u16 = 4;

#[derive(Clone, Debug)]
pub struct PreparedWater {
    pub field: FieldPyramid,
    pub pages: Vec<WaterPage>,
}

/// Rasterizes verified Natural Earth land polygons in the map's local
/// projection. Its complement is coarse ocean coverage; lakes and rivers are
/// deliberately reserved for separate source layers.
pub fn prepare_ocean_coverage(
    archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
    inland_coverage: Vec<u8>,
) -> Result<PreparedWater, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "working water grid is outside direct bounds",
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
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)?;
    let alias = temporary_zip_alias(archive)?;
    let path = alias
        .to_str()
        .ok_or(GeodataError::Preparation("water archive path is not UTF-8"))?;
    let vector = Dataset::open_ex(
        format!("/vsizip/{path}"),
        DatasetOptions {
            open_flags: GdalOpenFlags::GDAL_OF_VECTOR,
            ..DatasetOptions::default()
        },
    )?;
    let geometries = {
        let mut layer = vector.layer(0)?;
        layer
            .features()
            .filter_map(|feature| {
                feature
                    .geometry()
                    .map(|geometry| geometry.transform(&transform))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(vector);
    fs::remove_file(alias)?;
    if geometries.is_empty() {
        return Err(GeodataError::Preparation(
            "coastline source has no land polygons",
        ));
    }
    let axis = samples_per_axis
        .checked_mul(COASTLINE_SUPERSAMPLE)
        .ok_or(GeodataError::Preparation("water supersample overflows"))?;
    let side = estimate.effective_side_meters as f64;
    let spacing = side / f64::from(axis);
    let driver = DriverManager::get_driver_by_name("MEM")?;
    let mut raster =
        driver.create_with_band_type::<u8, _>("", usize::from(axis), usize::from(axis), 1)?;
    raster.set_geo_transform(&[-side / 2.0, spacing, 0.0, side / 2.0, 0.0, -spacing])?;
    raster.set_spatial_ref(&target)?;
    rasterize(
        &mut raster,
        &[1],
        &geometries,
        &vec![100.0; geometries.len()],
        None,
    )?;
    let land = raster
        .rasterband(1)?
        .read_as::<u8>(
            (0, 0),
            (usize::from(axis), usize::from(axis)),
            (usize::from(axis), usize::from(axis)),
            None,
        )?
        .data()
        .to_vec();
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut level_axis = samples_per_axis;
    let mut ocean = ocean_coverage(samples_per_axis, axis, &land)?;
    let mut inland = inland_on_land(&ocean, inland_coverage)?;
    loop {
        let level_pages = pages_for(levels.len() as u8, level_axis, &ocean, &inland)?;
        levels.push(PyramidLevel {
            samples_per_axis: level_axis,
            ordered_page_root: ordered_water_page_root(&level_pages)?,
        });
        pages.extend(level_pages);
        if level_axis == 1 {
            break;
        }
        ocean = reduce_coverage(level_axis, &ocean)?;
        inland = reduce_coverage(level_axis, &inland)?;
        level_axis = level_axis.div_ceil(2);
    }
    Ok(PreparedWater {
        field: FieldPyramid { levels },
        pages,
    })
}

fn temporary_zip_alias(archive: &Path) -> Result<PathBuf, GeodataError> {
    static NEXT_ALIAS: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_ALIAS.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let parent = archive.parent().ok_or(GeodataError::Preparation(
        "water archive has no parent directory",
    ))?;
    let alias = parent.join(format!(".aoe-geodata-{nanos}-{serial}.zip"));
    fs::hard_link(archive, &alias)?;
    Ok(alias)
}

fn ocean_coverage(axis: u16, supersampled_axis: u16, land: &[u8]) -> Result<Vec<u8>, GeodataError> {
    if land.len() != usize::from(supersampled_axis).pow(2) {
        return Err(GeodataError::Preparation(
            "coastline raster shape is invalid",
        ));
    }
    let factor = usize::from(supersampled_axis / axis);
    let mut values = Vec::with_capacity(usize::from(axis).pow(2));
    for y in 0..usize::from(axis) {
        for x in 0..usize::from(axis) {
            let mut land_total = 0_u32;
            for sample_y in y * factor..(y + 1) * factor {
                for sample_x in x * factor..(x + 1) * factor {
                    land_total +=
                        u32::from(land[sample_y * usize::from(supersampled_axis) + sample_x]);
                }
            }
            let count = u32::try_from(factor.pow(2))
                .map_err(|_| GeodataError::Preparation("water supersample count overflows"))?;
            values.push((100 - (land_total / count)) as u8);
        }
    }
    Ok(values)
}

fn inland_on_land(ocean: &[u8], inland: Vec<u8>) -> Result<Vec<u8>, GeodataError> {
    if inland.len() != ocean.len() {
        return Err(GeodataError::Preparation(
            "inland water grid shape is invalid",
        ));
    }
    Ok(ocean
        .iter()
        .zip(inland)
        .map(|(&ocean, inland)| if ocean == 0 { inland } else { 0 })
        .collect())
}

fn pages_for(
    level: u8,
    axis: u16,
    ocean: &[u8],
    inland: &[u8],
) -> Result<Vec<WaterPage>, GeodataError> {
    if ocean.len() != usize::from(axis).pow(2) || inland.len() != ocean.len() {
        return Err(GeodataError::Preparation("water grid shape is invalid"));
    }
    let mut pages = Vec::new();
    for y in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
        for x in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
            let width = (axis - x).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let height = (axis - y).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let mut ocean_coverage_percent =
                Vec::with_capacity(usize::from(width) * usize::from(height));
            let mut inland_coverage_percent = Vec::with_capacity(ocean_coverage_percent.capacity());
            for row in y..y + height {
                let start = usize::from(row) * usize::from(axis) + usize::from(x);
                let end = start + usize::from(width);
                ocean_coverage_percent.extend_from_slice(&ocean[start..end]);
                inland_coverage_percent.extend_from_slice(&inland[start..end]);
            }
            pages.push(WaterPage {
                level,
                x: x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                y: y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                width: width as u8,
                height: height as u8,
                ocean_coverage_percent,
                inland_coverage_percent,
            });
        }
    }
    Ok(pages)
}

fn reduce_coverage(axis: u16, values: &[u8]) -> Result<Vec<u8>, GeodataError> {
    let next_axis = axis.div_ceil(2);
    let mut reduced = Vec::with_capacity(usize::from(next_axis).pow(2));
    for y in 0..next_axis {
        for x in 0..next_axis {
            let mut total = 0_u32;
            let mut count = 0_u32;
            for source_y in y * 2..((y + 1) * 2).min(axis) {
                for source_x in x * 2..((x + 1) * 2).min(axis) {
                    total += u32::from(
                        values[usize::from(source_y) * usize::from(axis) + usize::from(source_x)],
                    );
                    count += 1;
                }
            }
            reduced.push((total / count) as u8);
        }
    }
    Ok(reduced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdal::{DriverManager, spatial_ref::SpatialRef, vector::LayerOptions};
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn coastline_supersampling_preserves_partial_ocean_coverage() {
        let land = [
            0, 0, 100, 100, 0, 0, 100, 100, 0, 0, 100, 100, 0, 0, 100, 100,
        ];
        assert_eq!(
            ocean_coverage(2, 4, &land).expect("coverage"),
            [100, 0, 100, 0]
        );
        assert!(ocean_coverage(2, 4, &land[..15]).is_err());
    }

    #[test]
    fn inland_coverage_never_overwrites_ocean() {
        assert_eq!(
            inland_on_land(&[100, 0], vec![100, 100]).expect("inland coverage"),
            [0, 100]
        );
        assert!(inland_on_land(&[0], vec![100, 0]).is_err());
    }

    #[test]
    fn native_vector_zip_is_rasterized_and_pyramided() {
        let root = temporary_directory();
        fs::create_dir_all(&root).expect("temporary directory");
        let geojson = root.join("land.geojson");
        let driver = DriverManager::get_driver_by_name("GeoJSON").expect("GeoJSON driver");
        let mut dataset = driver
            .create_vector_only(&geojson)
            .expect("GeoJSON dataset");
        let wgs84 = SpatialRef::from_epsg(4326).expect("WGS84");
        {
            let mut layer = dataset
                .create_layer(LayerOptions {
                    name: "land",
                    srs: Some(&wgs84),
                    ty: gdal::vector::OGRwkbGeometryType::wkbPolygon,
                    ..Default::default()
                })
                .expect("land layer");
            layer
                .create_feature(
                    gdal::vector::Geometry::from_wkt(
                        "POLYGON ((2.0 48.0, 2.35 48.0, 2.35 50.0, 2.0 50.0, 2.0 48.0))",
                    )
                    .expect("land polygon"),
                )
                .expect("land feature");
        }
        dataset.flush_cache().expect("flush GeoJSON");
        drop(dataset);

        let archive = root.join("land.zip");
        let mut zip = ZipWriter::new(fs::File::create(&archive).expect("vector archive"));
        zip.start_file("land.geojson", SimpleFileOptions::default())
            .expect("vector member");
        zip.write_all(&fs::read(&geojson).expect("GeoJSON bytes"))
            .expect("vector bytes");
        zip.finish().expect("finish vector archive");

        let prepared =
            prepare_ocean_coverage(&archive, MapRequest::default(), 2, vec![60, 60, 60, 60])
                .expect("prepared coastline");
        assert_eq!(prepared.field.levels.len(), 2);
        assert_eq!(prepared.pages.len(), 2);
        let page = &prepared.pages[0];
        assert!(page.ocean_coverage_percent.contains(&0));
        assert!(page.ocean_coverage_percent.contains(&100));
        assert!(page.inland_coverage_percent.contains(&60));
        assert!(
            page.ocean_coverage_percent
                .iter()
                .zip(&page.inland_coverage_percent)
                .any(|(ocean, inland)| *ocean == 0 && *inland == 60)
        );
        fs::remove_dir_all(root).expect("remove vector fixture");
    }

    fn temporary_directory() -> PathBuf {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("aoe-water-test-{}-{serial}", std::process::id()))
    }
}
