use super::{COPY_BUFFER_BYTES, CacheError};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub(super) type FileHashes = ([u8; 32], [u8; 20], [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fingerprint {
    length: u64,
    device: u64,
    inode: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[derive(Clone, Copy)]
struct CachedHashes {
    fingerprint: Fingerprint,
    hashes: FileHashes,
}

static VERIFIED_HASHES: OnceLock<Mutex<HashMap<PathBuf, CachedHashes>>> = OnceLock::new();

pub(super) fn file_hashes(path: &Path) -> Result<FileHashes, CacheError> {
    let before = fingerprint(path)?;
    let cache = VERIFIED_HASHES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(path)
        .copied()
        .filter(|cached| cached.fingerprint == before)
    {
        return Ok(cached.hashes);
    }
    if let Some(hashes) = stamped_hashes(path, &before) {
        remember(path, before, hashes);
        return Ok(hashes);
    }
    let mut file = File::open(path)?;
    let mut sha256 = Sha256::new();
    let mut sha1 = Sha1::new();
    let mut md5 = Md5::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha256.update(&buffer[..count]);
        sha1.update(&buffer[..count]);
        md5.update(&buffer[..count]);
    }
    let hashes = (
        sha256.finalize().into(),
        sha1.finalize().into(),
        md5.finalize().into(),
    );
    if fingerprint(path)? != before {
        return Err(CacheError::Integrity("file changed while hashing"));
    }
    write_stamp(path, &before, &hashes);
    remember(path, before, hashes);
    Ok(hashes)
}

fn remember(path: &Path, fingerprint: Fingerprint, hashes: FileHashes) {
    let mut cache = VERIFIED_HASHES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.len() >= 512
        && !cache.contains_key(path)
        && let Some(evicted) = cache.keys().next().cloned()
    {
        cache.remove(&evicted);
    }
    cache.insert(
        path.to_path_buf(),
        CachedHashes {
            fingerprint,
            hashes,
        },
    );
}

// Hashing a multi-gigabyte cached object takes tens of seconds, and every map
// job starts a fresh process. After one full hash, the digests are kept next to
// the object, bound to its length, inode and modification and status-change
// times: any write to the file changes mtime and ctime, so a stale stamp never
// matches. Only files in a cache `objects` directory are stamped.
fn stamp_path(path: &Path) -> Option<PathBuf> {
    let objects = path.parent()?;
    (objects.file_name()? == "objects").then(|| {
        objects
            .parent()
            .unwrap_or(objects)
            .join("verified")
            .join(path.file_name().unwrap_or_default())
    })
}

fn stamp_line(fingerprint: &Fingerprint) -> String {
    format!(
        "{} {} {}.{} {}.{}",
        fingerprint.length,
        fingerprint.inode,
        fingerprint.modified_seconds,
        fingerprint.modified_nanoseconds,
        fingerprint.changed_seconds,
        fingerprint.changed_nanoseconds,
    )
}

fn stamped_hashes(path: &Path, fingerprint: &Fingerprint) -> Option<FileHashes> {
    let text = fs::read_to_string(stamp_path(path)?).ok()?;
    let mut lines = text.lines();
    if lines.next()? != stamp_line(fingerprint) {
        return None;
    }
    let mut digest = |bytes: &mut [u8]| -> Option<()> {
        let hex = lines.next()?;
        (hex.len() == bytes.len() * 2).then_some(())?;
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(())
    };
    let (mut sha256, mut sha1, mut md5) = ([0; 32], [0; 20], [0; 16]);
    digest(&mut sha256)?;
    digest(&mut sha1)?;
    digest(&mut md5)?;
    // The object's name is its SHA-256; a stamp for other content is refused.
    (path.file_name()?.to_str()? == digest_hex(&sha256)).then_some((sha256, sha1, md5))
}

fn write_stamp(path: &Path, fingerprint: &Fingerprint, hashes: &FileHashes) {
    let Some(stamp) = stamp_path(path) else {
        return;
    };
    let text = format!(
        "{}\n{}\n{}\n{}\n",
        stamp_line(fingerprint),
        digest_hex(&hashes.0),
        digest_hex(&hashes.1),
        digest_hex(&hashes.2)
    );
    // Best effort: a read-only cache just hashes again next time.
    let temporary = stamp.with_extension(format!("{}.tmp", std::process::id()));
    let _ = stamp
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&temporary, text))
        .and_then(|()| fs::rename(&temporary, &stamp));
    let _ = fs::remove_file(&temporary);
}

fn fingerprint(path: &Path) -> Result<Fingerprint, CacheError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CacheError::Integrity(
            "content-addressed object is not a regular file",
        ));
    }
    #[cfg(unix)]
    {
        Ok(Fingerprint {
            length: metadata.len(),
            device: metadata.dev(),
            inode: metadata.ino(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
    #[cfg(not(unix))]
    {
        let modified = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(Fingerprint {
            length: metadata.len(),
            device: 0,
            inode: 0,
            modified_seconds: modified.as_secs() as i64,
            modified_nanoseconds: i64::from(modified.subsec_nanos()),
            changed_seconds: 0,
            changed_nanoseconds: 0,
        })
    }
}

pub(crate) fn digest_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn cached_hashes_recompute_after_content_changes() {
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aoe-geodata-digest-{}-{sequence}",
            std::process::id()
        ));
        fs::write(&path, b"abc").expect("initial file");
        let initial = file_hashes(&path).expect("initial digests");
        assert_eq!(initial, file_hashes(&path).expect("cached digests"));
        fs::write(&path, b"def").expect("changed file");
        assert_ne!(initial, file_hashes(&path).expect("changed digests"));
        fs::remove_file(path).expect("remove file");
    }

    #[test]
    fn stamped_digests_survive_a_new_process_and_die_with_the_content() {
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aoe-geodata-stamp-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("objects")).expect("objects");
        let temporary = root.join("objects/seed");
        fs::write(&temporary, b"abc").expect("seed");
        let name = digest_hex(&file_hashes(&temporary).expect("seed digests").0);
        let path = root.join("objects").join(&name);
        fs::rename(&temporary, &path).expect("name by digest");
        let first = file_hashes(&path).expect("first digests");
        assert!(root.join("verified").join(&name).is_file());
        // A new process starts with an empty in-memory cache.
        VERIFIED_HASHES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .expect("cache")
            .clear();
        let fingerprint = fingerprint(&path).expect("fingerprint");
        assert_eq!(stamped_hashes(&path, &fingerprint), Some(first));
        // Same length, new bytes: the status-change time moves, the stamp misses.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&path, b"abd").expect("rewrite");
        let changed = self::fingerprint(&path).expect("changed fingerprint");
        assert_eq!(stamped_hashes(&path, &changed), None);
        assert_ne!(first, file_hashes(&path).expect("recomputed"));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
