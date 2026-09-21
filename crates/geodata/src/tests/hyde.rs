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
