//! Observable WorldCover non-opening for vector-only sampling tests.
//!
//! A non-TIFF file with the canonical WorldCover name: vector-only sampling
//! must never open a WorldCover raster, so any attempt to open this decoy
//! fails GDAL and fails the pipeline under test.
use super::TestDirectory;
use std::fs;

const DECOY: &[u8] = b"vector-only decoy: opening a WorldCover raster here is a bug";

pub(super) fn write_worldcover_decoy(directory: &TestDirectory) {
    fs::write(directory.path("worldcover.tif"), DECOY).expect("write WorldCover decoy");
}

/// The decoy must still be in place after sampling: consumed, renamed or
/// truncated files would also mean the pipeline touched WorldCover data.
pub(super) fn assert_worldcover_decoy_survives(directory: &TestDirectory) {
    let bytes = fs::read(directory.path("worldcover.tif"))
        .expect("the WorldCover decoy must survive vector-only sampling");
    assert_eq!(
        bytes, DECOY,
        "the WorldCover decoy must survive vector-only sampling byte for byte"
    );
}
