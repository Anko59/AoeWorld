//! Landscape overview elevation averages Copernicus GLO-90 into each sample.
use super::*;
use crate::{Provider, SourceLock, elevation::PreparedElevation};
use aoe_map::Ratio;
use gdal::{DriverManager, raster::Buffer, spatial_ref::SpatialRef};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const AXIS: usize = 1_200;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aoe-overview-dem-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("scratch directory");
        Self(root)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A 1 degree GLO-90 stand-in whose pixel (x, y) holds `height(x, y)` meters.
fn write_tile(
    root: &Path,
    latitude: i32,
    longitude: i32,
    height: impl Fn(usize, usize) -> f32,
) -> Tile {
    write_shifted_tile(root, latitude, longitude, (0.0, 0.0), height)
}

/// As [`write_tile`], with the grid moved by `(east, north)` pixels. Real
/// GLO-90 tiles are point-registered: GDAL reports them half a pixel west and
/// north of their named degree, i.e. `(-0.5, 0.5)`.
fn write_shifted_tile(
    root: &Path,
    latitude: i32,
    longitude: i32,
    shift: (f64, f64),
    height: impl Fn(usize, usize) -> f32,
) -> Tile {
    let path = root.join(format!("tile-{latitude}-{longitude}.tif"));
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f32, _>(&path, AXIS, AXIS, 1)
        .expect("tile raster");
    let step = 1.0 / AXIS as f64;
    dataset
        .set_geo_transform(&[
            f64::from(longitude) + shift.0 * step,
            step,
            0.0,
            f64::from(latitude + 1) + shift.1 * step,
            0.0,
            -step,
        ])
        .expect("tile geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("tile spatial reference");
    let values = (0..AXIS * AXIS)
        .map(|index| height(index % AXIS, index / AXIS))
        .collect();
    dataset
        .rasterband(1)
        .expect("tile band")
        .write((0, 0), (AXIS, AXIS), &mut Buffer::new((AXIS, AXIS), values))
        .expect("tile values");
    dataset.flush_cache().expect("flush tile");
    let bytes = fs::read(&path).expect("tile bytes");
    Tile {
        latitude,
        longitude,
        path,
        lock: SourceLock {
            id: format!("fixture-glo90-{latitude}-{longitude}"),
            provider: Provider::Copernicus,
            release: "synthetic GLO-90 tile".to_owned(),
            url: "https://copernicus-dem-90m.s3.amazonaws.com/fixture/fixture.tif".to_owned(),
            sha256: crate::source_cache::digest_hex(&Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            native_resolution: "3 arc-seconds".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "EGM2008 orthometric".to_owned(),
            license_reference: "test fixture".to_owned(),
        },
    }
}

/// A 1.85 km ETOPO stand-in holding one value over longitudes -2..3.
fn write_coarse(root: &Path, meters: f64) -> PathBuf {
    let path = root.join("etopo.tif");
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(&path, 300, 300, 1)
        .expect("coarse raster");
    dataset
        .set_geo_transform(&[-2.0, 5.0 / 300.0, 0.0, 3.0, 0.0, -5.0 / 300.0])
        .expect("coarse geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("coarse spatial reference");
    dataset
        .rasterband(1)
        .expect("coarse band")
        .write(
            (0, 0),
            (300, 300),
            &mut Buffer::new((300, 300), vec![meters; 300 * 300]),
        )
        .expect("coarse values");
    dataset.flush_cache().expect("flush coarse");
    path
}

/// An ETOPO stand-in over longitudes 0..6 and latitudes 47..53.
fn write_coarse_north(root: &Path, meters: f64) -> PathBuf {
    let path = root.join("etopo-north.tif");
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(&path, 360, 360, 1)
        .expect("coarse raster");
    dataset
        .set_geo_transform(&[0.0, 6.0 / 360.0, 0.0, 53.0, 0.0, -6.0 / 360.0])
        .expect("coarse geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("coarse spatial reference");
    dataset
        .rasterband(1)
        .expect("coarse band")
        .write(
            (0, 0),
            (360, 360),
            &mut Buffer::new((360, 360), vec![meters; 360 * 360]),
        )
        .expect("coarse values");
    dataset.flush_cache().expect("flush coarse");
    path
}

fn request(latitude_e7: i32, longitude_e7: i32, side_meters: u64) -> MapRequest {
    MapRequest {
        center_latitude_e7: latitude_e7,
        center_longitude_e7: longitude_e7,
        requested_side_meters: side_meters,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    }
}

fn level_zero(elevation: &PreparedElevation) -> Vec<i32> {
    let axis = usize::from(elevation.environment.samples_per_axis);
    let mut grid = vec![0; axis * axis];
    for page in elevation.pages.iter().filter(|page| page.level == 0) {
        for row in 0..usize::from(page.height) {
            for column in 0..usize::from(page.width) {
                let y = usize::from(page.y) * 64 + row;
                let x = usize::from(page.x) * 64 + column;
                grid[y * axis + x] =
                    page.geographic_height_centimeters[row * usize::from(page.width) + column];
            }
        }
    }
    grid
}

fn tile_count(bounds: Bounds) -> usize {
    bounds.tile_count()
}

fn paris(side_meters: u64, compression: u32) -> MapRequest {
    MapRequest {
        requested_side_meters: side_meters,
        compression: Ratio::new(compression, 1).expect("ratio"),
        ..MapRequest::default()
    }
}

#[test]
fn overview_dem_plan_covers_large_but_not_giant_footprints() {
    let large = plan_overview_tiles(paris(480_000, 30))
        .expect("large plan")
        .expect("a Large map fits the tile budget");
    assert!(
        (30..=64).contains(&tile_count(large)),
        "Large Paris needs {} tiles",
        tile_count(large)
    );
    let tiny = plan_overview_tiles(paris(30_000, 30))
        .expect("tiny plan")
        .expect("a Tiny map fits the tile budget");
    assert!(tile_count(tiny) <= 4);
    // Meridians converge toward the pole: Large still fits at 65 degrees
    // north, but not at 80, where it would need over 150 tiles.
    for (latitude_e7, fits) in [(650_000_000, true), (800_000_000, false)] {
        let plan = plan_overview_tiles(MapRequest {
            center_latitude_e7: latitude_e7,
            center_longitude_e7: 250_000_000,
            ..paris(480_000, 30)
        })
        .expect("high latitude plan");
        assert_eq!(plan.is_some(), fits, "latitude {latitude_e7}");
    }
    assert!(
        plan_overview_tiles(paris(1_920_000, 30))
            .expect("giant plan")
            .is_none(),
        "Giant keeps the ETOPO overview"
    );
    assert!(
        plan_overview_tiles(request(899_000_000, 0, 120_000))
            .expect("polar plan")
            .is_none(),
        "a footprint at the pole keeps the ETOPO overview"
    );
}

#[test]
fn overview_elevation_averages_subsample_relief_instead_of_point_sampling() {
    let scratch = Scratch::new();
    // Alternating 0 m and 100 m pixels: a point sample reads one of the two,
    // the area mean of each 2.5 km cell is 50 m.
    let tile = write_tile(
        &scratch.0,
        0,
        0,
        |x, y| {
            if (x + y) % 2 == 0 { 0.0 } else { 100.0 }
        },
    );
    let coarse = write_coarse(&scratch.0, 700.0);
    let elevation =
        prepare_area_averaged_elevation(&coarse, request(5_000_000, 5_000_000, 20_000), 8, &[tile])
            .expect("area-averaged elevation");
    let grid = level_zero(&elevation);
    assert_eq!(grid.len(), 64);
    for (index, height) in grid.iter().enumerate() {
        assert!(
            (4_900..=5_100).contains(height),
            "sample {index} is {height} cm, not the 5000 cm cell mean"
        );
    }
}

#[test]
fn overview_elevation_tracks_a_gradient_at_sample_scale() {
    let scratch = Scratch::new();
    // Height equals the pixel column in meters (one pixel is 92.8 m wide).
    let tile = write_tile(&scratch.0, 0, 0, |x, _| x as f32);
    let coarse = write_coarse(&scratch.0, 700.0);
    let elevation =
        prepare_area_averaged_elevation(&coarse, request(5_000_000, 5_000_000, 20_000), 8, &[tile])
            .expect("gradient elevation");
    let grid = level_zero(&elevation);
    let first_row = &grid[..8];
    assert!(first_row.windows(2).all(|pair| pair[1] > pair[0]));
    // Seven cell widths of 2.5 km span 17.5 km, or 188.6 pixels.
    let rise = f64::from(first_row[7] - first_row[0]) / 100.0;
    assert!((rise - 188.6).abs() < 2.0, "the gradient rises {rise} m");
    // The centre of the grid sits at pixel column 600, mean height 599.5 m.
    let middle = f64::from(first_row[3] + first_row[4]) / 200.0;
    assert!((middle - 599.5).abs() < 2.0, "the middle reads {middle} m");
}

#[test]
fn overview_elevation_takes_uncovered_area_from_the_coarse_source() {
    let scratch = Scratch::new();
    let coarse = write_coarse(&scratch.0, 700.0);
    let side = request(5_000_000, 10_000_000, 20_000);
    // No tile at all: the coarse source supplies every sample.
    let none = prepare_area_averaged_elevation(&coarse, side, 8, &[]).expect("coarse only");
    assert!(level_zero(&none).iter().all(|height| *height == 70_000));
    // A tile west of longitude 1 degree: the east half is uncovered.
    let tile = write_tile(&scratch.0, 0, 0, |_, _| 300.0);
    let mixed = prepare_area_averaged_elevation(&coarse, side, 8, &[tile]).expect("mixed");
    let grid = level_zero(&mixed);
    for row in grid.chunks(8) {
        assert!(row[..3].iter().all(|height| *height == 30_000), "{row:?}");
        assert!(row[5..].iter().all(|height| *height == 70_000), "{row:?}");
    }
}

#[test]
fn overview_elevation_accepts_a_half_pixel_registration_and_rejects_a_misnamed_tile() {
    let scratch = Scratch::new();
    let coarse = write_coarse(&scratch.0, 700.0);
    let side = request(5_000_000, 5_000_000, 20_000);
    let point = write_shifted_tile(&scratch.0, 0, 0, (-0.5, 0.0), |_, _| 300.0);
    let elevation =
        prepare_area_averaged_elevation(&coarse, side, 8, &[point]).expect("point registration");
    assert!(
        level_zero(&elevation)
            .iter()
            .all(|height| *height == 30_000)
    );
    // A tile filed under the wrong degree is refused rather than misplaced.
    let mut misnamed = write_tile(&scratch.0, 0, 0, |_, _| 300.0);
    misnamed.longitude = 1;
    assert!(
        prepare_area_averaged_elevation(
            &coarse,
            request(5_000_000, 15_000_000, 20_000),
            8,
            &[misnamed]
        )
        .is_err()
    );
}

fn seam_sampler<'a>(coarse: &Path, tiles: &'a [Tile]) -> AreaSampler<'a> {
    AreaSampler::new(
        coarse,
        request(500_000_000, 25_000_000, 20_000),
        20_000.0,
        tiles,
    )
    .expect("sampler")
}

#[test]
fn overview_cell_across_a_degree_line_takes_both_point_registered_tiles() {
    let scratch = Scratch::new();
    let coarse = write_coarse_north(&scratch.0, 700.0);
    let shift = (-0.5, 0.5);
    let tiles = [
        write_shifted_tile(&scratch.0, 50, 2, shift, |_, _| 300.0),
        write_shifted_tile(&scratch.0, 50, 3, shift, |_, _| 300.0),
    ];
    let mut sampler = seam_sampler(&coarse, &tiles);
    // E003 starts half a pixel west of longitude 3: this cell lies across
    // that seam while its box still ends before the whole degree.
    let mean = sampler
        .mean(2.9995, 50.5, 0.0001, 0.0001)
        .expect("seam mean");
    assert!((mean - 300.0).abs() < 1e-6, "longitude seam reads {mean} m");
}

#[test]
fn overview_cell_across_a_latitude_degree_takes_both_point_registered_tiles() {
    let scratch = Scratch::new();
    let coarse = write_coarse_north(&scratch.0, 700.0);
    let shift = (-0.5, 0.5);
    let tiles = [
        write_shifted_tile(&scratch.0, 49, 2, shift, |_, _| 300.0),
        write_shifted_tile(&scratch.0, 50, 2, shift, |_, _| 300.0),
    ];
    let mut sampler = seam_sampler(&coarse, &tiles);
    // N49 ends half a pixel north of latitude 50.
    let mean = sampler
        .mean(2.5, 50.0004, 0.0001, 0.0001)
        .expect("seam mean");
    assert!((mean - 300.0).abs() < 1e-6, "latitude seam reads {mean} m");
    let inside = sampler
        .mean(2.5, 50.0002, 0.00005, 0.00005)
        .expect("inside N49");
    assert!(
        (inside - 300.0).abs() < 1e-6,
        "N49 interior reads {inside} m"
    );
}

#[test]
fn overview_sea_surface_zeros_keep_the_coarse_bathymetry_and_low_land_stays() {
    let side = request(5_000_000, 5_000_000, 20_000);
    let read = |coarse_meters: f64, height: &dyn Fn(usize, usize) -> f32| {
        let scratch = Scratch::new();
        let tile = write_tile(&scratch.0, 0, 0, height);
        let coarse = write_coarse(&scratch.0, coarse_meters);
        level_zero(&prepare_area_averaged_elevation(&coarse, side, 8, &[tile]).expect("coastal"))
    };
    // GLO-90 writes the sea as 0 m where ETOPO knows the depth.
    let sea = read(-50.0, &|_, _| 0.0);
    assert!(sea.iter().all(|height| *height == -5_000), "{sea:?}");
    // Low-lying land that really is 0 m, with ETOPO above the sea.
    let marsh = read(2.0, &|_, _| 0.0);
    assert!(marsh.iter().all(|height| *height == 0), "{marsh:?}");
    // Land at 1.5 m beside sea zeros under a negative ETOPO cell: the land
    // pixels stay, the zeros take the depth, so the cell mean is between.
    let shore = read(-40.0, &|x, y| if (x + y) % 2 == 0 { 0.0 } else { 1.5 });
    assert!(
        shore
            .iter()
            .all(|height| (-1_990..=-1_860).contains(height)),
        "{shore:?}"
    );
}
