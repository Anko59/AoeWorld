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
    let mut cache = cache
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
            fingerprint: before,
            hashes,
        },
    );
    Ok(hashes)
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

pub(super) fn digest_hex(digest: &[u8]) -> String {
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
}
