//! Package navigation is streamed once off tick, then queried through immutable
//! file-backed views. Published inodes are never modified, only replaced.
use aoe_map::{MapChunkGenerator, MapPackage, NAVIGATION_FORMAT_VERSION, NavigationSnapshot};
use aoe_simulation::{GameWorld, Terrain};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const MAGIC: &[u8; 8] = b"AOENAV02";
const HEADER_BYTES: usize = 44;
const INDEX_RECORD_BYTES: usize = 44;
const MAX_CHUNK_BYTES: usize = 4 << 20;
// Reserve room for simulation, terrain/source caches, sessions and bake scratch
// within the server's 5 GiB envelope. Mapped pages are shared and evictable.
const MAX_FILE_BYTES: u64 = 2 << 30;
const MAX_INDEX_HEAP_BYTES: usize = 16 << 20;
mod background;
mod immutable;
mod mapped;
mod shared;
mod stream;
use mapped::read;
pub(crate) use shared::{Usage as SnapshotUsage, usage as snapshot_usage};

fn chunk_side(generator: &MapChunkGenerator) -> Option<usize> {
    let side = usize::try_from(generator.width_tiles())
        .ok()?
        .div_ceil(aoe_map::CHUNK_TILES as usize);
    let count = side.checked_mul(side)?;
    (count > 0 && count <= MAX_INDEX_HEAP_BYTES / size_of::<std::ops::Range<usize>>())
        .then_some(side)
}

fn navigation_path(root: &Path, hash: [u8; 32]) -> PathBuf {
    root.join("navigation").join(format!(
        "{}-{}.bin",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        NAVIGATION_FORMAT_VERSION
    ))
}

pub(crate) fn prepare_world(
    world: &GameWorld,
    hash: [u8; 32],
    directory: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    if cancelled() {
        return Err("navigation preparation cancelled".to_owned());
    }
    if let Terrain::Map { generator, .. } = world.terrain() {
        if generator.has_static_navigation() || chunk_side(generator).is_none() {
            return Ok(());
        }
        if let Some(root) = directory {
            let path = navigation_path(root, hash);
            if path.exists() {
                return load(
                    generator,
                    &path,
                    hash,
                    chunk_side(generator).ok_or("navigation index exceeds budget")?,
                    cancelled,
                );
            }
        }
        background::launch(generator, hash, directory);
    }
    Ok(())
}

pub(crate) fn prepare_package(
    package: &MapPackage,
    directory: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let generator = if package.environment.samples_per_axis == 0 {
        package.generator()
    } else {
        let root = directory.ok_or("prepared navigation needs package storage")?;
        let provider =
            super::PageResidency::open(root, package, cancelled).map_err(|e| e.to_string())?;
        package
            .generator_with_page_provider(provider)
            .map_err(|e| e.to_string())?
    };
    prepare(&generator, package.content_hash, directory, cancelled)
}

fn load(
    generator: &MapChunkGenerator,
    path: &Path,
    hash: [u8; 32],
    side: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    // A regular cache file may need a sealed copy. Recheck sharing under one
    // off-tick loader lock so simultaneous worlds never copy a package twice.
    static LOADING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _loading = LOADING.lock().unwrap_or_else(|e| e.into_inner());
    let key = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let snapshot = match shared::get(&key) {
        Some(snapshot) => snapshot,
        None => read(path, hash, side, cancelled)?,
    };
    if let Err(error) = generator.install_navigation_snapshot(snapshot.clone())
        && !generator.has_static_navigation()
    {
        return Err(error.to_string());
    }
    shared::insert(key, snapshot);
    Ok(())
}

fn prepare(
    generator: &MapChunkGenerator,
    hash: [u8; 32],
    directory: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    if cancelled() {
        return Err("navigation preparation cancelled".to_owned());
    }
    if generator.has_static_navigation() {
        return Ok(());
    }
    let Some(side) = chunk_side(generator) else {
        tracing::info!("navigation index exceeds bounded budget; using lazy navigation");
        return Ok(());
    };
    let started = Instant::now();
    let scratch = if directory.is_none() {
        Some(tempfile::tempdir().map_err(|e| e.to_string())?)
    } else {
        None
    };
    let root = directory
        .or_else(|| scratch.as_ref().map(|dir| dir.path()))
        .ok_or("missing navigation directory")?;
    let path = navigation_path(root, hash);
    fs::create_dir_all(path.parent().ok_or("navigation path has no parent")?)
        .map_err(|e| e.to_string())?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if cancelled() {
        return Err("navigation preparation cancelled".to_owned());
    }
    if generator.has_static_navigation() {
        return Ok(());
    }
    if !path.exists() {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(side * side)
            .min(4);
        stream::build(
            generator,
            side,
            workers,
            &path,
            hash,
            MAX_FILE_BYTES,
            cancelled,
        )?;
    }
    // A temporary package still uses mapping, but must not pin its deleted
    // pathname in the process LRU after the caller releases the snapshot.
    if directory.is_some() {
        load(generator, &path, hash, side, cancelled)?;
    } else {
        generator
            .install_navigation_snapshot(read(&path, hash, side, cancelled)?)
            .map_err(|e| e.to_string())?;
    }
    let snapshot = generator
        .navigation_snapshot()
        .ok_or("missing navigation snapshot")?;
    tracing::info!(
        chunks = snapshot.chunk_count(),
        retained_bytes = snapshot.retained_bytes(),
        heap_bytes = snapshot.heap_bytes(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "static navigation mapped off tick"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
