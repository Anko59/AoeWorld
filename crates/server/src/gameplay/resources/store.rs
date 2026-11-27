//! Checkpoints hold resource amounts and the recovery schedule at a given tick.
//! A separate 16-byte clock record advances pending timers without scanning the overlay.
//! Its revision and tick identify which checkpoint it can extend.

use super::ResourceLifecycleError;
use aoe_map::ResourceOverlaySnapshot;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

// Worst-case JSON change and recovery entries plus metadata.
// The bound covers the map's supported overlay, independently of wire frames.
pub(super) const MAX_SNAPSHOT_BYTES: u64 = aoe_map::MAX_RESOURCE_OVERLAY_CHANGES as u64 * 96 + 1024;
static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn path(directory: &Path, hash: [u8; 32]) -> PathBuf {
    let name: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    directory.join(format!("{name}.json"))
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Checkpoint<S = ResourceOverlaySnapshot> {
    pub(super) snapshot: S,
    pub(super) tick: u64,
}

pub(super) fn load(
    directory: &Path,
    hash: [u8; 32],
) -> Result<Option<Checkpoint>, ResourceLifecycleError> {
    let _guard = SAVE_LOCK.lock().map_err(|_| ResourceLifecycleError::Task)?;
    let Some(mut checkpoint) = load_checkpoint(directory, hash)? else {
        return Ok(None);
    };
    if !checkpoint.snapshot.recovery.is_empty()
        && let Some((revision, tick)) = load_clock(directory, hash)?
        && revision == checkpoint.snapshot.revision
        && tick > checkpoint.tick
    {
        let elapsed = tick - checkpoint.tick;
        for (remaining, _) in &mut checkpoint.snapshot.recovery {
            *remaining = remaining
                .checked_sub(elapsed)
                .ok_or(aoe_map::ResourceOverlayError::InvalidSnapshot)?;
        }
        checkpoint.tick = tick;
    }
    Ok(Some(checkpoint))
}

fn load_checkpoint(
    directory: &Path,
    hash: [u8; 32],
) -> Result<Option<Checkpoint>, ResourceLifecycleError> {
    let file = match File::open(path(directory, hash)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if file.metadata()?.len() > MAX_SNAPSHOT_BYTES {
        return Err(ResourceLifecycleError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(ResourceLifecycleError::TooLarge);
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

/// Returns false only when replacement succeeded but directory fsync failed.
/// Callers MUST commit live state after replacement, including this outcome.
pub(super) fn save(
    directory: &Path,
    snapshot: &ResourceOverlaySnapshot,
    previous_revision: u64,
    tick: u64,
) -> Result<bool, ResourceLifecycleError> {
    // One authoritative server process owns the package directory. Serialize
    // same-process services too, rejecting a retired world's stale revision.
    let _guard = SAVE_LOCK.lock().map_err(|_| ResourceLifecycleError::Task)?;
    let stored = load_checkpoint(directory, snapshot.map_content_hash)?;
    if stored
        .as_ref()
        .is_some_and(|value| value.snapshot.map_content_hash != snapshot.map_content_hash)
    {
        return Err(aoe_map::ResourceOverlayError::WrongMap.into());
    }
    if stored.as_ref().map_or(0, |value| value.snapshot.revision) != previous_revision
        || stored.as_ref().is_some_and(|value| value.tick > tick)
        || load_clock(directory, snapshot.map_content_hash)?
            .is_some_and(|(revision, at)| (revision, at) > (previous_revision, tick))
    {
        return Err(ResourceLifecycleError::StaleRevision);
    }
    let bytes = serde_json::to_vec(&Checkpoint { snapshot, tick })?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(ResourceLifecycleError::TooLarge);
    }
    replace(
        directory,
        &path(directory, snapshot.map_content_hash),
        &bytes,
    )
}

fn clock_path(directory: &Path, hash: [u8; 32]) -> PathBuf {
    path(directory, hash).with_extension("clock")
}

fn load_clock(
    directory: &Path,
    hash: [u8; 32],
) -> Result<Option<(u64, u64)>, ResourceLifecycleError> {
    let mut file = match File::open(clock_path(directory, hash)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut revision = [0; 8];
    let mut tick = [0; 8];
    file.read_exact(&mut revision)?;
    file.read_exact(&mut tick)?;
    if file.read(&mut [0])? != 0 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into());
    }
    Ok(Some((
        u64::from_le_bytes(revision),
        u64::from_le_bytes(tick),
    )))
}

/// Save elapsed recovery time without serializing or scanning the overlay.
/// A clock from an older amount revision is superseded by its checkpoint.
pub(super) fn save_clock(
    directory: &Path,
    hash: [u8; 32],
    revision: u64,
    tick: u64,
) -> Result<bool, ResourceLifecycleError> {
    let _guard = SAVE_LOCK.lock().map_err(|_| ResourceLifecycleError::Task)?;
    if load_clock(directory, hash)?.is_some_and(|stored| stored > (revision, tick)) {
        return Err(ResourceLifecycleError::StaleRevision);
    }
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&revision.to_le_bytes());
    bytes[8..].copy_from_slice(&tick.to_le_bytes());
    replace(directory, &clock_path(directory, hash), &bytes)
}

fn replace(
    directory: &Path,
    destination: &Path,
    bytes: &[u8],
) -> Result<bool, ResourceLifecycleError> {
    fs::create_dir_all(directory)?;
    // A fixed name bounds crash leftovers to one per immutable map. The
    // process lock means an existing temp cannot belong to an active writer.
    let temporary = destination.with_extension("resource-tmp");
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, destination)?;
        Ok::<_, std::io::Error>(
            File::open(directory)
                .and_then(|directory| directory.sync_all())
                .is_ok(),
        )
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(result?)
}
