use super::{
    GeodataError, HydrologyKind, MAX_PAGE_FEATURES, MAX_PAGE_GEOMETRY_BYTES, OpenTile, PAGE,
    WORLD_COVER_NODATA,
};
use aoe_map::MapRequest;
use gdal::{
    Dataset, DatasetOptions, GdalOpenFlags,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
    vector::{FieldValue, LayerAccess},
};
use std::collections::BTreeMap;
use std::path::Path;

#[path = "sampling/river_geometry.rs"]
mod river_geometry;
pub(super) use river_geometry::{RiverReachGeometry, line_position, river_reach_geometry};

const MAX_WORLD_COVER_WINDOW_PIXELS: usize = 4 * 1024 * 1024;
const WORLD_COVER_BLOCK_PIXELS: usize = 2_048;
type PixelGroups = BTreeMap<(usize, usize), Vec<(usize, usize, usize)>>;

pub(super) struct Bounds {
    pub(super) min_latitude: f64,
    pub(super) max_latitude: f64,
    pub(super) min_longitude: f64,
    pub(super) max_longitude: f64,
}

pub(super) fn request_bounds(request: MapRequest) -> Result<Bounds, GeodataError> {
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    let points = crate::projected_footprint(request, crate::MAX_FOOTPRINT_SAMPLES_PER_EDGE)?;
    let min_lat = points
        .iter()
        .map(|point| f64::from(point.latitude_e7) / 10_000_000.0)
        .fold(f64::INFINITY, f64::min);
    let max_lat = points
        .iter()
        .map(|point| f64::from(point.latitude_e7) / 10_000_000.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lon = points
        .iter()
        .map(|point| f64::from(point.longitude_e7) / 10_000_000.0)
        .fold(f64::INFINITY, f64::min);
    let max_lon = points
        .iter()
        .map(|point| f64::from(point.longitude_e7) / 10_000_000.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = estimate.effective_side_meters as f64 / 100_000.0 + 0.1;
    Ok(Bounds {
        min_latitude: (min_lat - margin).max(-90.0),
        max_latitude: (max_lat + margin).min(82.75),
        min_longitude: (min_lon - margin).max(-180.0),
        max_longitude: (max_lon + margin).min(180.0),
    })
}

fn check_window_bound(width: usize, height: usize) -> Result<(), GeodataError> {
    if width
        .checked_mul(height)
        .is_none_or(|area| area > MAX_WORLD_COVER_WINDOW_PIXELS)
    {
        return Err(GeodataError::Preparation(
            "WorldCover page read window exceeds its memory bound",
        ));
    }
    Ok(())
}

pub(super) struct VectorFeature {
    pub(super) geometry: gdal::vector::Geometry,
    pub(super) kind: HydrologyKind,
    pub(super) river_reach: Option<RiverReachGeometry>,
}

pub(super) fn page_bounds(
    east: &[f64],
    north: &[f64],
) -> Result<(f64, f64, f64, f64), GeodataError> {
    if east.is_empty() || east.len() != north.len() {
        return Err(GeodataError::Preparation(
            "hydrology page has no coordinates",
        ));
    }
    let min_x = east.iter().copied().fold(f64::INFINITY, f64::min);
    let max_x = east.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_y = north.iter().copied().fold(f64::INFINITY, f64::min);
    let max_y = north.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let latitude_padding = 0.01;
    let longitude_padding = 0.01;
    Ok((
        min_x - longitude_padding,
        min_y - latitude_padding,
        max_x + longitude_padding,
        max_y + latitude_padding,
    ))
}

pub(super) fn open_vector_source(archive: &Path, member: &str) -> Result<Dataset, GeodataError> {
    let path = format!("/vsizip/{{{}}}/{}", archive.display(), member);
    let dataset = Dataset::open_ex(
        &path,
        DatasetOptions {
            open_flags: GdalOpenFlags::GDAL_OF_VECTOR,
            ..DatasetOptions::default()
        },
    )?;
    {
        let layer = dataset.layer(0)?;
        let spatial_ref = layer
            .spatial_ref()
            .ok_or(GeodataError::Preparation("hydrology vector CRS is missing"))?;
        if spatial_ref.auth_name().as_deref() != Some("EPSG") || spatial_ref.auth_code()? != 4326 {
            return Err(GeodataError::Preparation(
                "hydrology vector CRS is not EPSG:4326",
            ));
        }
    }
    Ok(dataset)
}

pub(super) fn sample_worldcover_page(
    tiles: &[OpenTile],
    longitudes: &[f64],
    latitudes: &[f64],
) -> Result<Vec<u8>, GeodataError> {
    if longitudes.len() != latitudes.len() {
        return Err(GeodataError::Preparation(
            "WorldCover coordinate shape is invalid",
        ));
    }
    let mut values = vec![WORLD_COVER_NODATA; longitudes.len()];
    let mut covered = vec![false; longitudes.len()];
    for tile in tiles {
        let spatial_ref = tile.dataset.spatial_ref()?;
        if spatial_ref.auth_name().as_deref() != Some("EPSG") || spatial_ref.auth_code()? != 4326 {
            return Err(GeodataError::Preparation(
                "WorldCover raster CRS is not EPSG:4326",
            ));
        }
        let transform = tile.dataset.geo_transform()?;
        if transform[2] != 0.0 || transform[4] != 0.0 || transform[1] <= 0.0 || transform[5] >= 0.0
        {
            return Err(GeodataError::Preparation(
                "WorldCover geotransform is not north-up",
            ));
        }
        let (raster_width, raster_height) = tile.dataset.raster_size();
        let mut pixels = Vec::new();
        for (index, (&longitude, &latitude)) in longitudes.iter().zip(latitudes).enumerate() {
            if latitude < f64::from(tile.latitude)
                || latitude > f64::from(tile.latitude + 3)
                || longitude < f64::from(tile.longitude)
                || longitude > f64::from(tile.longitude + 3)
            {
                continue;
            }
            let pixel_x = ((longitude - transform[0]) / transform[1]).floor() as isize;
            let pixel_y = ((latitude - transform[3]) / transform[5]).floor() as isize;
            if pixel_x >= 0
                && pixel_y >= 0
                && (pixel_x as usize) < raster_width
                && (pixel_y as usize) < raster_height
            {
                pixels.push((index, pixel_x as usize, pixel_y as usize));
            }
        }
        if pixels.is_empty() {
            continue;
        }
        let mut groups = PixelGroups::new();
        for (index, pixel_x, pixel_y) in pixels {
            groups
                .entry((
                    pixel_x / WORLD_COVER_BLOCK_PIXELS,
                    pixel_y / WORLD_COVER_BLOCK_PIXELS,
                ))
                .or_default()
                .push((index, pixel_x, pixel_y));
        }
        for pixels in groups.into_values() {
            let min_x = pixels.iter().map(|(_, x, _)| *x).min().unwrap_or(0);
            let max_x = pixels.iter().map(|(_, x, _)| *x).max().unwrap_or(0);
            let min_y = pixels.iter().map(|(_, _, y)| *y).min().unwrap_or(0);
            let max_y = pixels.iter().map(|(_, _, y)| *y).max().unwrap_or(0);
            let width = max_x - min_x + 1;
            let height = max_y - min_y + 1;
            check_window_bound(width, height)?;
            let block = tile.dataset.rasterband(1)?.read_as::<u8>(
                (min_x as isize, min_y as isize),
                (width, height),
                (width, height),
                None,
            )?;
            for (index, pixel_x, pixel_y) in pixels {
                let value = block.data()[(pixel_y - min_y) * width + pixel_x - min_x];
                if covered[index] && values[index] != value {
                    return Err(GeodataError::Preparation(
                        "overlapping WorldCover tiles disagree at a sample",
                    ));
                }
                values[index] = value;
                covered[index] = true;
            }
        }
    }
    if covered.iter().any(|sampled| !sampled) {
        return Err(GeodataError::Preparation(
            "selected WorldCover tiles do not cover every requested coordinate",
        ));
    }
    if values.iter().any(|class| {
        !matches!(
            *class,
            0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 95 | 100
        )
    }) {
        return Err(GeodataError::Preparation(
            "WorldCover raster contains an unknown class value",
        ));
    }
    Ok(values)
}

pub(super) fn flatten_ocean_pages(
    axis: u16,
    pages: &[aoe_map::WaterPage],
) -> Result<Vec<u8>, GeodataError> {
    let expected = usize::from(axis).pow(2);
    let mut values = vec![0; expected];
    let mut occupied = vec![false; expected];
    for page in pages {
        if page.width == 0 || page.height == 0 {
            return Err(GeodataError::Preparation("ocean page has zero dimensions"));
        }
        if page.ocean_coverage_percent.len() != usize::from(page.width) * usize::from(page.height) {
            return Err(GeodataError::Preparation("ocean page shape is invalid"));
        }
        let origin_x = usize::from(page.x) * usize::from(PAGE);
        let origin_y = usize::from(page.y) * usize::from(PAGE);
        let expected_width = (axis.saturating_sub(origin_x as u16)).min(PAGE) as u8;
        let expected_height = (axis.saturating_sub(origin_y as u16)).min(PAGE) as u8;
        if page.width != expected_width || page.height != expected_height {
            return Err(GeodataError::Preparation(
                "ocean page dimensions are inconsistent",
            ));
        }
        for row in 0..usize::from(page.height) {
            let destination = (origin_y + row) * usize::from(axis) + origin_x;
            let source = row * usize::from(page.width);
            if destination + usize::from(page.width) > values.len() {
                return Err(GeodataError::Preparation("ocean page exceeds grid bounds"));
            }
            values[destination..destination + usize::from(page.width)].copy_from_slice(
                &page.ocean_coverage_percent[source..source + usize::from(page.width)],
            );
            for covered in occupied
                .iter_mut()
                .skip(destination)
                .take(usize::from(page.width))
            {
                if *covered {
                    return Err(GeodataError::Preparation("ocean pages overlap"));
                }
                *covered = true;
            }
        }
    }
    if occupied.iter().any(|covered| !covered) {
        return Err(GeodataError::Preparation(
            "ocean pages do not cover the grid",
        ));
    }
    Ok(values)
}

pub(super) fn resample_ocean_coverage(
    source_axis: u16,
    target_axis: u16,
    pages: &[aoe_map::WaterPage],
) -> Result<Vec<u8>, GeodataError> {
    if !(2..=16_384).contains(&source_axis) || !(2..=1_024).contains(&target_axis) {
        return Err(GeodataError::Preparation(
            "ocean resampling axes are outside supported bounds",
        ));
    }
    let base_pages = pages
        .iter()
        .filter(|page| page.level == 0)
        .cloned()
        .collect::<Vec<_>>();
    let source = flatten_ocean_pages(source_axis, &base_pages)?;
    let mut target = Vec::with_capacity(usize::from(target_axis).pow(2));
    for y in 0..target_axis {
        let source_y = (((u32::from(y) * 2 + 1) * u32::from(source_axis))
            / (u32::from(target_axis) * 2))
            .min(u32::from(source_axis - 1)) as usize;
        for x in 0..target_axis {
            let source_x = (((u32::from(x) * 2 + 1) * u32::from(source_axis))
                / (u32::from(target_axis) * 2))
                .min(u32::from(source_axis - 1)) as usize;
            target.push(source[source_y * usize::from(source_axis) + source_x]);
        }
    }
    Ok(target)
}

pub(super) fn vector_features(
    dataset: &mut Dataset,
    bounds: (f64, f64, f64, f64),
    definition: &str,
    rivers: bool,
) -> Result<Vec<VectorFeature>, GeodataError> {
    let mut layer = dataset.layer(0)?;
    layer.set_spatial_filter_rect(bounds.0, bounds.1, bounds.2, bounds.3);
    let transform = local_transform(definition)?;
    let mut features = Vec::new();
    let mut geometry_bytes = 0_usize;
    // gdal-rs queries the feature count while constructing this iterator.
    // OpenFileGDB can consume the filtered cursor during that query, even
    // with force=false. Rewind afterward through another handle to this layer.
    let iterator = layer.features();
    dataset.layer(0)?.reset_feature_reading();
    for feature in iterator {
        if features.len() >= MAX_PAGE_FEATURES {
            return Err(GeodataError::Preparation(
                "hydrology page has too many vector features",
            ));
        }
        let Some(geometry) = feature.geometry() else {
            continue;
        };
        let transformed = geometry.transform(&transform)?;
        let source_bytes = transformed.wkb()?.len();
        let kind = if rivers {
            HydrologyKind::River
        } else {
            let index = feature.field_index("Lake_type")?;
            match feature.field(index)? {
                Some(FieldValue::IntegerValue(1)) => HydrologyKind::Lake,
                Some(FieldValue::IntegerValue(2)) => HydrologyKind::Reservoir,
                Some(FieldValue::IntegerValue(3)) => HydrologyKind::RegulatedLake,
                _ => HydrologyKind::UnknownWater,
            }
        };
        let river_reach = if rivers {
            river_reach_geometry(&feature, &transformed)
        } else {
            None
        };
        let line_bytes = river_reach
            .as_ref()
            .map(|reach| {
                reach
                    .line_points
                    .len()
                    .checked_mul(std::mem::size_of::<(f64, f64)>())
                    .ok_or(GeodataError::Preparation(
                        "HydroRIVERS line geometry size overflows",
                    ))
            })
            .transpose()?
            .unwrap_or(0);
        let geometry = if rivers {
            transformed.buffer(river_width(&feature)?, 4)?
        } else {
            transformed
        };
        let retained_bytes = geometry.wkb()?.len();
        let retained_and_line_bytes =
            retained_bytes
                .checked_add(line_bytes)
                .ok_or(GeodataError::Preparation(
                    "hydrology page geometry exceeds its memory bound",
                ))?;
        geometry_bytes = checked_geometry_bytes(
            geometry_bytes,
            if rivers { source_bytes } else { 0 },
            retained_and_line_bytes,
        )?;
        features.push(VectorFeature {
            geometry,
            kind,
            river_reach,
        });
    }
    Ok(features)
}

fn local_transform(definition: &str) -> Result<CoordTransform, GeodataError> {
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    let mut target =
        SpatialRef::from_definition(definition).map_err(|_| GeodataError::Projection)?;
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)
}

fn river_width(feature: &gdal::vector::Feature<'_>) -> Result<f64, GeodataError> {
    let index = feature.field_index("DIS_AV_CMS")?;
    let discharge = match feature.field(index)? {
        Some(FieldValue::RealValue(value)) if value.is_finite() => value,
        Some(FieldValue::IntegerValue(value)) => f64::from(value),
        _ => 0.0,
    };
    Ok((4.0 + discharge.max(0.0).sqrt() * 3.0).clamp(4.0, 100.0))
}

fn checked_geometry_bytes(
    retained: usize,
    source: usize,
    buffered: usize,
) -> Result<usize, GeodataError> {
    retained
        .checked_add(source)
        .and_then(|value| value.checked_add(buffered))
        .filter(|value| *value <= MAX_PAGE_GEOMETRY_BYTES)
        .ok_or(GeodataError::Preparation(
            "hydrology page geometry exceeds its memory bound",
        ))
}

pub(super) fn tile_latitude(id: &str) -> Result<i32, GeodataError> {
    Ok(parse_worldcover_coordinates(id)?.0)
}

pub(super) fn tile_longitude(id: &str) -> Result<i32, GeodataError> {
    Ok(parse_worldcover_coordinates(id)?.1)
}

fn parse_worldcover_coordinates(id: &str) -> Result<(i32, i32), GeodataError> {
    let name = id
        .strip_prefix("worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_")
        .and_then(|value| value.strip_suffix("_Map.tif"))
        .ok_or(GeodataError::Preparation("invalid WorldCover source id"))?;
    let bytes = name.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .filter(|value| value.iter().all(u8::is_ascii_digit))
            .and_then(|value| std::str::from_utf8(value).ok())
            .and_then(|value| value.parse::<i32>().ok())
    };
    if bytes.len() != 7 || !matches!(bytes[0], b'N' | b'S') || !matches!(bytes[3], b'E' | b'W') {
        return Err(GeodataError::Preparation(
            "invalid WorldCover tile coordinates",
        ));
    }
    let latitude = digits(1..3).ok_or(GeodataError::Preparation("invalid WorldCover latitude"))?;
    let longitude =
        digits(4..7).ok_or(GeodataError::Preparation("invalid WorldCover longitude"))?;
    if latitude > 90
        || longitude > 180
        || (latitude == 0 && bytes[0] == b'S')
        || (longitude == 0 && bytes[3] == b'W')
    {
        return Err(GeodataError::Preparation("WorldCover tile is out of range"));
    }
    Ok((
        if bytes[0] == b'S' {
            -latitude
        } else {
            latitude
        },
        if bytes[3] == b'W' {
            -longitude
        } else {
            longitude
        },
    ))
}

#[cfg(test)]
#[path = "tests/sampling.rs"]
mod tests;
