use super::ResourceLifecycleError;
use aoe_map::ResourceOverlaySnapshot;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_SNAPSHOT_BYTES: u64 = 4 * 1024 * 1024;
static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn path(directory: &Path, hash: [u8; 32]) -> PathBuf {
    let name: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    directory.join(format!("{name}.json"))
}

pub(super) fn load(
    directory: &Path,
    hash: [u8; 32],
) -> Result<Option<ResourceOverlaySnapshot>, ResourceLifecycleError> {
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
) -> Result<bool, ResourceLifecycleError> {
    // One authoritative server process owns the package directory. Serialize
    // same-process services too, rejecting a retired world's stale revision.
    let _guard = SAVE_LOCK.lock().map_err(|_| ResourceLifecycleError::Task)?;
    let stored = load(directory, snapshot.map_content_hash)?;
    if stored
        .as_ref()
        .is_some_and(|value| value.map_content_hash != snapshot.map_content_hash)
    {
        return Err(aoe_map::ResourceOverlayError::WrongMap.into());
    }
    if stored.as_ref().map_or(0, |value| value.revision) != previous_revision {
        return Err(ResourceLifecycleError::StaleRevision);
    }
    let bytes = serde_json::to_vec(snapshot)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(ResourceLifecycleError::TooLarge);
    }
    fs::create_dir_all(directory)?;
    // A fixed name bounds crash leftovers to one per immutable map. The
    // process lock means an existing temp cannot belong to an active writer.
    let destination = path(directory, snapshot.map_content_hash);
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
        file.write_all(&bytes)?;
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
