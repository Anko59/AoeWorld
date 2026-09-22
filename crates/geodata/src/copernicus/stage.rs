use super::{GeodataError, MAX_DETAILED_STAGING_BYTES};
use crate::directory::{MAX_DIRECTORY_PAGE_BYTES, publish_streaming_page};
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, MapPackage, PageLayer};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PAGE: u16 = ENVIRONMENT_PAGE_SAMPLES as u16;

pub(super) struct Stage {
    root: PathBuf,
    bytes: AtomicU64,
}

impl Stage {
    pub(super) fn lease(root: &Path) -> Result<fs::File, GeodataError> {
        let path = root.join("lease");
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(GeodataError::Preparation(
                "worker staging lease is not a regular file",
            ));
        }
        let lease = fs::OpenOptions::new().read(true).write(true).open(&path)?;
        lease
            .try_lock_shared()
            .map_err(|_| GeodataError::Preparation("worker staging is being recovered"))?;
        // A recovery pass may have removed the directory before this lock was
        // acquired. Never start writing against an already-unlinked lease.
        if !path.is_file() {
            return Err(GeodataError::Preparation(
                "worker staging was recovered before acquiring its lease",
            ));
        }
        Ok(lease)
    }

    pub(super) fn new(cache_root: &Path) -> Result<Self, GeodataError> {
        let parent = cache_root.join("detailed-staging");
        fs::create_dir_all(&parent)?;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        for _ in 0..32 {
            let id = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("{}-{}-{id}", std::process::id(), unix_nanos()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        root: path,
                        bytes: AtomicU64::new(0),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(GeodataError::Preparation(
            "could not allocate detailed staging directory",
        ))
    }

    fn path(&self, layer: PageLayer, level: u8, x: u16, y: u16) -> PathBuf {
        self.root
            .join(layer.directory_name())
            .join(format!("{level}-{x}-{y}.json"))
    }

    pub(super) fn write(
        &self,
        layer: PageLayer,
        level: u8,
        x: u16,
        y: u16,
        bytes: &[u8],
    ) -> Result<(), GeodataError> {
        let next = self
            .bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed)
            .saturating_add(bytes.len() as u64);
        if bytes.len() as u64 > MAX_DIRECTORY_PAGE_BYTES || next > MAX_DETAILED_STAGING_BYTES {
            return Err(GeodataError::Preparation("detailed staging exceeds 2 GiB"));
        }
        let path = self.path(layer, level, x, y);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    pub(super) fn read(
        &self,
        layer: PageLayer,
        level: u8,
        x: u16,
        y: u16,
    ) -> Result<Vec<u8>, GeodataError> {
        let mut bytes = Vec::new();
        fs::File::open(self.path(layer, level, x, y))?
            .take(MAX_DIRECTORY_PAGE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DIRECTORY_PAGE_BYTES {
            return Err(GeodataError::Directory(
                "staged page exceeds its byte limit".to_owned(),
            ));
        }
        Ok(bytes)
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(super) fn publish_staged_pages(
    stage: &Stage,
    output: &Path,
    package: &MapPackage,
    samples_per_axis: u16,
) -> Result<(), GeodataError> {
    let hash = package.content_hash_hex();
    if package.environment.samples_per_axis != samples_per_axis {
        return Err(GeodataError::Preparation(
            "staged package axis does not match its manifest",
        ));
    }
    publish_field(
        stage,
        output,
        &hash,
        PageLayer::Elevation,
        &package.environment.elevation.levels,
    )?;
    publish_field(
        stage,
        output,
        &hash,
        PageLayer::Water,
        &package
            .environment
            .water
            .as_ref()
            .ok_or(GeodataError::Preparation("detailed water field is missing"))?
            .levels,
    )?;
    publish_field(
        stage,
        output,
        &hash,
        PageLayer::Vegetation,
        &package
            .environment
            .vegetation
            .as_ref()
            .ok_or(GeodataError::Preparation(
                "detailed vegetation field is missing",
            ))?
            .levels,
    )?;
    publish_field(
        stage,
        output,
        &hash,
        PageLayer::HistoricalLandUse,
        &package
            .environment
            .historical_land_use
            .as_ref()
            .ok_or(GeodataError::Preparation(
                "detailed historical field is missing",
            ))?
            .levels,
    )?;
    Ok(())
}

fn publish_field(
    stage: &Stage,
    output: &Path,
    hash: &str,
    layer: PageLayer,
    levels: &[aoe_map::PyramidLevel],
) -> Result<(), GeodataError> {
    for (level, metadata) in levels.iter().enumerate() {
        let count = usize::from(metadata.samples_per_axis.div_ceil(PAGE));
        for y in 0..count {
            for x in 0..count {
                let level = u8::try_from(level)
                    .map_err(|_| GeodataError::Preparation("page level overflows"))?;
                let bytes = stage.read(layer, level, x as u16, y as u16)?;
                publish_streaming_page(output, hash, layer, level, x as u16, y as u16, &bytes)?;
            }
        }
    }
    Ok(())
}
