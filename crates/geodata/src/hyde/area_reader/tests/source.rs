use super::*;
use aoe_map::{MapRequest, Ratio};
use gdal::DriverManager;
use std::{
    fs::{self, File},
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn fixed_fiji_page_uses_two_bounded_source_windows_across_the_dateline() {
    let archive = global_mask_fixture();
    let request = MapRequest {
        center_latitude_e7: -178_000_000,
        center_longitude_e7: 1_798_000_000,
        requested_side_meters: 80_000,
        compression: Ratio::new(80, 1).expect("valid compression"),
        ..MapRequest::default()
    };
    let transform = super::super::target_to_wgs84(request).expect("projection transform");
    let side = request
        .estimate()
        .expect("valid Fiji footprint")
        .effective_side_meters;
    let (targets, _) = super::super::target_page(
        &transform,
        side,
        80,
        super::super::PageBounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        },
        179.8,
    )
    .expect("transformed Fiji target page");
    let longitudes = targets
        .iter()
        .flat_map(|target| &target.polygon)
        .map(|point| point.longitude_degrees)
        .collect::<Vec<_>>();
    assert!(longitudes.iter().any(|longitude| *longitude > 180.0));
    assert!(
        longitudes
            .iter()
            .all(|longitude| (179.0..181.0).contains(longitude))
    );

    let raster =
        RasterSource::open(&archive.path, HYDE_600_MEMBERS[3]).expect("open global mask fixture");
    let windows = raster
        .windows_for_targets(&targets)
        .expect("split Fiji target page");
    assert_eq!(windows.len(), 2);
    assert!(
        windows
            .iter()
            .map(|window| window.pixels.width)
            .sum::<usize>()
            < 10
    );
    assert!(
        windows
            .iter()
            .map(|window| window.pixels.width * window.pixels.height)
            .sum::<usize>()
            < 100
    );
    assert_ne!(
        windows[0].longitude_offset_degrees,
        windows[1].longitude_offset_degrees
    );
}

#[test]
fn rounded_hyde_world_grid_uses_exact_five_minute_seam_bounds() {
    let rounded_cell = HYDE_600_ROUNDED_CELL_DEGREES;
    let rounded_transform = [
        -180.0,
        rounded_cell,
        0.0,
        -90.0 + HYDE_600_HEIGHT as f64 * rounded_cell,
        0.0,
        -rounded_cell,
    ];
    let driver = DriverManager::get_driver_by_name("MEM").expect("MEM driver");
    let mut dataset = driver
        .create_with_band_type::<u8, _>("", HYDE_600_WIDTH, HYDE_600_HEIGHT, 1)
        .expect("in-memory world raster");
    dataset
        .set_geo_transform(&rounded_transform)
        .expect("rounded HYDE transform");
    let (width, height) = dataset.raster_size();
    let transform = canonical_hyde_600_transform(
        width,
        height,
        dataset.geo_transform().expect("read transform"),
    );
    assert_eq!(transform[1], 1.0 / 12.0);
    assert_eq!(transform[3], 90.0);
    assert_eq!(transform[5], -1.0 / 12.0);

    let inverse = transform.invert().expect("exact-grid inverse");
    let source = RasterSource {
        dataset,
        width,
        height,
        transform,
        inverse,
        nodata: None,
    };
    let targets = [HydeTargetAreaCell {
        polygon: vec![
            HydeGeographicPoint {
                longitude_degrees: 179.9,
                latitude_degrees: 0.1,
            },
            HydeGeographicPoint {
                longitude_degrees: 180.1,
                latitude_degrees: 0.1,
            },
            HydeGeographicPoint {
                longitude_degrees: 180.1,
                latitude_degrees: -0.1,
            },
            HydeGeographicPoint {
                longitude_degrees: 179.9,
                latitude_degrees: -0.1,
            },
        ],
    }];
    let windows = source
        .windows_for_targets(&targets)
        .expect("split exact global grid at the dateline");

    assert_eq!(windows.len(), 2);
    assert_eq!(
        source.cell_polygon(HYDE_600_WIDTH - 1, 1_080, 0.0)[1].longitude_degrees,
        180.0
    );
    assert_eq!(
        source.cell_polygon(0, 1_080, 360.0)[0].longitude_degrees,
        180.0
    );
}

#[test]
fn reported_valid_land_area_uses_spherical_five_minute_capacity() {
    let north = 90.0 - 792.0 / 12.0;
    let south = north - 1.0 / 12.0;
    let area = spherical_cell_area_square_kilometers(1.0 / 12.0, north, south);

    assert!((area - 78.465_375_16).abs() < 1.0e-7);
    assert!(!valid_area_exceeds_capacity(78.4654, area));
    assert!(valid_area_exceeds_capacity(78.4655, area));
}

#[test]
fn ascii_grid_source_quantities_keep_float64_precision_and_restore_gdal_config() {
    let baseline = archive_with_members(
        "baseline",
        &[
            (HYDE_600_MEMBERS[0], &scalar_grid("57.4671039166")),
            (HYDE_600_MEMBERS[1], &scalar_grid("0.4951960834")),
            (HYDE_600_MEMBERS[2], &scalar_grid("20")),
        ],
    );
    let supplementary = archive_with_members(
        "supplementary",
        &[
            (HYDE_600_MEMBERS[3], &scalar_grid("1")),
            (HYDE_600_MEMBERS[4], &scalar_grid("57.9623")),
        ],
    );
    let previous = get_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "")
        .expect("read prior GDAL thread option");
    set_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "Float32")
        .expect("set prior GDAL thread option");

    let result = (|| {
        let reader = ArchiveReader::open(&baseline.path, &supplementary.path)?;
        let targets = [HydeTargetAreaCell {
            polygon: vec![
                HydeGeographicPoint {
                    longitude_degrees: 0.0,
                    latitude_degrees: 1.0,
                },
                HydeGeographicPoint {
                    longitude_degrees: 1.0,
                    latitude_degrees: 1.0,
                },
                HydeGeographicPoint {
                    longitude_degrees: 1.0,
                    latitude_degrees: 0.0,
                },
                HydeGeographicPoint {
                    longitude_degrees: 0.0,
                    latitude_degrees: 0.0,
                },
            ],
        }];
        let sources = reader.source_cells(&targets)?;
        let crop = sources[0]
            .crop_area_square_kilometers
            .expect("crop quantity");
        let grazing = sources[0]
            .grazing_area_square_kilometers
            .expect("grazing quantity");
        assert!((crop - 57.467_103_916_6).abs() < 1.0e-10);
        assert!((grazing - 0.495_196_083_4).abs() < 1.0e-10);
        assert!((crop + grazing - 57.9623).abs() < 1.0e-12);
        super::super::allocate_hyde_area_window(&sources, &targets, 0, 0)
            .expect("decimal source quantities fit their shared denominator");
        assert_eq!(
            get_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "")?,
            "Float32"
        );

        let invalid_grid = baseline.root.join("not-an-ascii-grid.asc");
        fs::write(&invalid_grid, "not a raster").expect("write invalid raster");
        assert!(open_aai_grid_float64(&invalid_grid).is_err());
        assert_eq!(
            get_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, "")?,
            "Float32"
        );
        Ok::<(), GeodataError>(())
    })();

    let restore = if previous.is_empty() {
        clear_thread_local_config_option(AAI_GRID_DATATYPE_OPTION)
    } else {
        set_thread_local_config_option(AAI_GRID_DATATYPE_OPTION, &previous)
    };
    restore.expect("restore original GDAL thread option");
    result.expect("read full-precision HYDE source quantities");
}

struct ArchiveFixture {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl Drop for ArchiveFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn global_mask_fixture() -> ArchiveFixture {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aoe-hyde-global-mask-{}-{timestamp}-{serial}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("fixture directory");
    let mut grid = String::from(
        "ncols 360\nnrows 180\nxllcorner -180\nyllcorner -90\ncellsize 1\nNODATA_value -9999\n",
    );
    let row = format!("{}\n", "1 ".repeat(360));
    for _ in 0..180 {
        grid.push_str(&row);
    }
    archive_with_root(root, "supplementary.zip", &[(HYDE_600_MEMBERS[3], &grid)])
}

fn archive_with_members(name: &str, members: &[(&str, &str)]) -> ArchiveFixture {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aoe-hyde-{name}-{}-{timestamp}-{serial}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("fixture directory");
    archive_with_root(root, &format!("{name}.zip"), members)
}

fn archive_with_root(
    root: std::path::PathBuf,
    filename: &str,
    members: &[(&str, &str)],
) -> ArchiveFixture {
    let path = root.join(filename);
    let mut output = ZipWriter::new(File::create(&path).expect("fixture archive"));
    for (member, grid) in members {
        output
            .start_file(*member, SimpleFileOptions::default())
            .expect("archive member");
        output.write_all(grid.as_bytes()).expect("archive grid");
    }
    output.finish().expect("finish archive");
    ArchiveFixture { root, path }
}

fn scalar_grid(value: &str) -> String {
    format!("ncols 1\nnrows 1\nxllcorner 0\nyllcorner 0\ncellsize 1\nNODATA_value -9999\n{value}\n")
}
