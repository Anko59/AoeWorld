use super::*;
use gdal::{DriverManager, raster::Buffer};
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn sampler_preserves_land_lake_nodata_nan_and_outside_as_distinct_results() {
    let archive = test_landlake_archive();
    let mut coordinates = vec![(0.5, 0.5); 25];
    coordinates[..5].copy_from_slice(&[(0.5, 0.5), (1.5, 0.5), (2.5, 0.5), (3.5, 0.5), (4.5, 0.5)]);
    let values = sample_member(&archive, HYDE_600_MEMBERS[3], &coordinates, 5)
        .expect("sampled HYDE fixture");

    assert_eq!(&values[..5], [Some(1.0), Some(0.0), None, None, None]);
    assert_eq!(
        values[..5]
            .iter()
            .copied()
            .map(lake_coverage_percent)
            .collect::<Vec<_>>(),
        vec![0, 100, 0, 0, 0]
    );
}

#[test]
fn prepares_hyde_600_from_bounded_ascii_zip_members() {
    let archives = test_hyde_archives();
    let prepared = prepare_hyde_600(
        &archives.baseline,
        &archives.supplementary,
        MapRequest::default(),
        2,
    )
    .expect("prepared HYDE fixture");

    assert_eq!(prepared.field.levels.len(), 2);
    assert_eq!(prepared.pages.len(), 2);
    let first = &prepared.pages[0];
    assert_eq!(first.width, 2);
    assert_eq!(first.height, 2);
    assert!(first.crop_percent.iter().all(|value| *value == 40));
    assert!(first.grazing_percent.iter().all(|value| *value == 30));
    assert!(
        first
            .population_pressure_per_square_kilometer
            .iter()
            .all(|value| *value == 2)
    );

    // The extracted members are reusable and do not require another archive read.
    let repeated = prepare_hyde_600(
        &archives.baseline,
        &archives.supplementary,
        MapRequest::default(),
        2,
    )
    .expect("reused HYDE extraction");
    assert_eq!(repeated.pages, prepared.pages);
}

#[test]
fn rejects_invalid_hyde_archives_and_member_paths() {
    let archives = test_hyde_archives();
    assert!(extract_member(&archives.baseline, "../../outside.asc").is_err());
    let invalid = archives.root.join("invalid.zip");
    fs::write(&invalid, b"not a ZIP archive").expect("invalid archive");
    assert!(matches!(
        prepare_hyde_lake_coverage(&invalid, MapRequest::default(), 2),
        Err(GeodataError::Preparation(_))
    ));
}

fn test_hyde_archives() -> TestArchives {
    static NEXT_ARCHIVE: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "aoe-hyde-full-test-{}-{serial}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let baseline = root.join("baseline.zip");
    let supplementary = root.join("supplementary-full.zip");
    write_zip(
        &baseline,
        &[
            (HYDE_600_MEMBERS[0], "4"),
            (HYDE_600_MEMBERS[1], "3"),
            (HYDE_600_MEMBERS[2], "20"),
        ],
    );
    write_zip(
        &supplementary,
        &[(HYDE_600_MEMBERS[3], "1"), (HYDE_600_MEMBERS[4], "10")],
    );
    TestArchives {
        root,
        baseline,
        supplementary,
    }
}

fn write_zip(path: &std::path::Path, members: &[(&str, &str)]) {
    let output = File::create(path).expect("test archive");
    let mut zip = ZipWriter::new(output);
    for (member, value) in members {
        zip.start_file(*member, SimpleFileOptions::default())
            .expect("archive member");
        zip.write_all(ascii_grid(value).as_bytes())
            .expect("archive bytes");
    }
    zip.finish().expect("finish archive");
}

fn ascii_grid(value: &str) -> String {
    let mut grid = String::from(
        "ncols 8\nnrows 8\nxllcorner 1\nyllcorner 47\ncellsize 0.5\nNODATA_value -9999\n",
    );
    for _ in 0..8 {
        grid.push_str(&format!(
            "{value} {value} {value} {value} {value} {value} {value} {value}\n"
        ));
    }
    grid
}

struct TestArchives {
    root: std::path::PathBuf,
    baseline: std::path::PathBuf,
    supplementary: std::path::PathBuf,
}

impl Drop for TestArchives {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn test_landlake_archive() -> TestArchive {
    static NEXT_TEST_ARCHIVE: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_TEST_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aoe-hyde-test-{}-{timestamp}-{serial}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("test directory");
    let source = root.join("landlake.tif");
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(&source, 4, 1, 1)
        .expect("landlake raster");
    dataset
        .set_geo_transform(&[0.0, 1.0, 0.0, 1.0, 0.0, -1.0])
        .expect("landlake geotransform");
    {
        let mut band = dataset.rasterband(1).expect("landlake band");
        band.set_no_data_value(Some(-9_999.0))
            .expect("landlake nodata");
        let mut values = Buffer::new((4, 1), vec![1.0, 0.0, -9_999.0, f64::NAN]);
        band.write((0, 0), (4, 1), &mut values)
            .expect("landlake values");
    }
    dataset.flush_cache().expect("flush landlake raster");
    drop(dataset);
    let bytes = fs::read(&source).expect("read landlake raster");
    let archive = root.join("supplementary.zip");
    let output = File::create(&archive).expect("test archive");
    let mut zip = ZipWriter::new(output);
    zip.start_file(HYDE_600_MEMBERS[3], SimpleFileOptions::default())
        .expect("landlake member");
    zip.write_all(&bytes).expect("landlake raster");
    zip.finish().expect("finish test archive");
    TestArchive { root, archive }
}

struct TestArchive {
    root: std::path::PathBuf,
    archive: std::path::PathBuf,
}

impl std::ops::Deref for TestArchive {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.archive
    }
}

impl Drop for TestArchive {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
