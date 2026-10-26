//! Package-derived navigation is built once, outside ticking. Files contain
//! only derived navigation, keyed by package identity and the current format.
use aoe_map::{MapChunkGenerator, MapPackage, NAVIGATION_FORMAT_VERSION, NavigationChunk};
use aoe_simulation::{GameWorld, Terrain};
use std::{
    fs,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAGIC: &[u8; 8] = b"AOENAV01";
const MAX_CHUNK_BYTES: usize = 4 << 20;
// Larger worlds use the existing lazy, bounded walk/portal caches. Never
// allocate an eager chunk vector in proportion to the creator's map area.
const MAX_EAGER_CHUNKS: usize = 65_536;
const MAX_SNAPSHOT_BYTES: usize = 512 << 20;
mod shared;
pub(crate) use shared::{Usage as SnapshotUsage, usage as snapshot_usage};

pub(crate) fn prepare_world(
    world: &GameWorld,
    hash: [u8; 32],
    directory: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    if let Terrain::Map { generator, .. } = world.terrain() {
        prepare(generator, hash, directory, cancelled)?;
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
    let started = Instant::now();
    let side = (generator.width_tiles() as usize).div_ceil(aoe_map::CHUNK_TILES as usize);
    if side * side > MAX_EAGER_CHUNKS {
        tracing::info!(chunks = side * side, "large map uses lazy navigation");
        return Ok(());
    }
    let path = directory.map(|root| {
        root.join("navigation").join(format!(
            "{}-{}.bin",
            hash.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            NAVIGATION_FORMAT_VERSION
        ))
    });
    // One bake per package, including concurrent jobs and server processes.
    // Drop releases the native file lock after publication or failure.
    let _preparation_lock = if let Some(path) = &path {
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
        Some(lock)
    } else {
        None
    };
    // Another preparation may have completed while this generator waited.
    if generator.has_static_navigation() {
        return Ok(());
    }
    if let Some(path) = &path
        && path.exists()
    {
        let key = fs::canonicalize(path).map_err(|e| e.to_string())?;
        if let Some(snapshot) = shared::get(&key) {
            let retained_bytes = snapshot.retained_bytes();
            generator
                .install_navigation_snapshot(snapshot)
                .map_err(|e| e.to_string())?;
            tracing::info!(retained_bytes, "static navigation shared before ticking");
            return Ok(());
        }
        let chunks = read(path, hash, side, cancelled)?;
        generator
            .install_static_navigation(chunks)
            .map_err(|e| e.to_string())?;
        if let Some(snapshot) = generator.navigation_snapshot() {
            shared::insert(key, snapshot);
        }
        tracing::info!(
            chunks = side * side,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "static navigation loaded before ticking"
        );
        return Ok(());
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(side * side)
        .min(4);
    tracing::info!(
        chunks = side * side,
        workers,
        "preparing complete static navigation before ticking"
    );
    let chunks = build(generator, side, workers, MAX_SNAPSHOT_BYTES, cancelled)?;
    if let Some(path) = &path {
        fs::create_dir_all(path.parent().ok_or("navigation path has no parent")?)
            .map_err(|e| e.to_string())?;
        write(path, hash, side, &chunks)?;
    }
    generator
        .install_static_navigation(chunks)
        .map_err(|e| e.to_string())?;
    if let Some(path) = &path
        && let Some(snapshot) = generator.navigation_snapshot()
    {
        shared::insert(fs::canonicalize(path).map_err(|e| e.to_string())?, snapshot);
    }
    tracing::info!(
        chunks = side * side,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "static navigation built before ticking"
    );
    Ok(())
}

fn build(
    generator: &MapChunkGenerator,
    side: usize,
    workers: usize,
    byte_limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<NavigationChunk>, String> {
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (send, receive) = mpsc::sync_channel(workers * 2);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (next, stop, send) = (&next, &stop, send.clone());
            let worker = generator.navigation_preparation_worker();
            scope.spawn(move || {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    if at >= side * side || stop.load(Ordering::Acquire) {
                        break;
                    }
                    let chunk = worker
                        .prepare_static_navigation_chunk(
                            (at % side) as i32,
                            (at / side) as i32,
                            &|| stop.load(Ordering::Acquire),
                        )
                        .map_err(|e| e.to_string());
                    if send.send(chunk).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut chunks = Vec::with_capacity(side * side);
        let mut retained_bytes = 0;
        let mut error = None;
        let mut last_progress = Instant::now();
        loop {
            if cancelled() {
                stop.store(true, Ordering::Release);
                error = Some("navigation preparation cancelled".to_owned());
            }
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(chunk)) if error.is_none() => {
                    retained_bytes += chunk.retained_bytes();
                    if retained_bytes > byte_limit {
                        stop.store(true, Ordering::Release);
                        error = Some("navigation memory budget exceeded; choose a smaller map or increase compression".to_owned());
                    } else {
                        chunks.push(chunk);
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(reason)) => {
                    stop.store(true, Ordering::Release);
                    error.get_or_insert(reason);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if last_progress.elapsed() >= Duration::from_secs(10) {
                tracing::info!(
                    complete = chunks.len(),
                    total = side * side,
                    "static navigation preparation progress"
                );
                last_progress = Instant::now();
            }
        }
        if let Some(error) = error {
            return Err(error);
        }
        if chunks.len() != side * side {
            return Err("incomplete static navigation".to_owned());
        }
        chunks.sort_unstable_by_key(|chunk| {
            let (x, y) = chunk.coordinates();
            (y, x)
        });
        Ok(chunks)
    })
}

fn read(
    path: &Path,
    hash: [u8; 32],
    side: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<NavigationChunk>, String> {
    let mut file = BufReader::new(fs::File::open(path).map_err(|e| e.to_string())?);
    let mut header = [0; 44];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..8] != MAGIC || header[8..40] != hash || header[40..] != (side as u32).to_le_bytes()
    {
        return Err("navigation package identity or dimensions differ".into());
    }
    let mut chunks = Vec::with_capacity(side * side);
    let mut retained_bytes = 0;
    for _ in 0..side * side {
        if cancelled() {
            return Err("navigation load cancelled".into());
        }
        let mut length = [0; 4];
        let mut checksum = [0; 32];
        file.read_exact(&mut length)
            .and_then(|_| file.read_exact(&mut checksum))
            .map_err(|e| e.to_string())?;
        let length = u32::from_le_bytes(length) as usize;
        if length > MAX_CHUNK_BYTES {
            return Err("navigation chunk exceeds limit".into());
        }
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        if aoe_map::navigation_checksum(&bytes) != checksum {
            return Err("navigation chunk checksum differs".into());
        }
        let chunk = NavigationChunk::decode(&bytes).map_err(|e| e.to_string())?;
        retained_bytes += chunk.retained_bytes();
        if retained_bytes > MAX_SNAPSHOT_BYTES {
            return Err(
                "navigation memory budget exceeded; choose a smaller map or increase compression"
                    .to_owned(),
            );
        }
        chunks.push(chunk);
        if chunks.len() % 25_000 == 0 || chunks.len() == side * side {
            tracing::info!(
                chunks = chunks.len(),
                retained_bytes,
                "navigation snapshot memory"
            );
        }
    }
    if file.read(&mut [0]).map_err(|e| e.to_string())? != 0 {
        return Err("navigation has trailing bytes".into());
    }
    Ok(chunks)
}

fn write(
    path: &Path,
    hash: [u8; 32],
    side: usize,
    chunks: &[NavigationChunk],
) -> Result<(), String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> io::Result<()> {
        let mut file = BufWriter::new(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?,
        );
        file.write_all(MAGIC)?;
        file.write_all(&hash)?;
        file.write_all(&(side as u32).to_le_bytes())?;
        for chunk in chunks {
            let bytes = chunk.encode().map_err(io::Error::other)?;
            file.write_all(&(bytes.len() as u32).to_le_bytes())?;
            file.write_all(&aoe_map::navigation_checksum(&bytes))?;
            file.write_all(&bytes)?;
        }
        file.flush()?;
        file.get_ref().sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(
            path.parent()
                .ok_or_else(|| io::Error::other("navigation path has no parent"))?,
        )?
        .sync_all()
    })();
    let _ = fs::remove_file(temporary);
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
