use super::{
    CacheError, SourceCache, SourceLock,
    digest::{digest_hex, file_hashes},
};
use crate::KnownSource;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

impl SourceCache {
    pub fn acquire_known(
        &self,
        source: &KnownSource,
        cancelled: &AtomicBool,
    ) -> Result<SourceLock, CacheError> {
        if source.id.is_empty()
            || source.bytes == 0
            || !source.provider.permits(&source.url)
            || !source.has_valid_acquisition_policy()
        {
            return Err(CacheError::InvalidLock("known source metadata is invalid"));
        }
        if let Some(lock) = self.cached_known(source)? {
            return Ok(lock);
        }
        if source.bytes > self.policy.job_acquisition_budget_bytes {
            return Err(CacheError::Budget(
                "source exceeds the per-job acquisition budget",
            ));
        }
        let partial = self.root.join("partial").join(format!(
            "known-{:x}.part",
            Sha256::digest(source.id.as_bytes())
        ));
        let usage = super::directory_bytes(&self.root)?;
        let partial_bytes = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
        if usage.saturating_add(source.bytes.saturating_sub(partial_bytes))
            > self.policy.cache_quota_bytes
        {
            return Err(CacheError::Budget(
                "source exceeds the remaining cache quota",
            ));
        }
        for attempt in 0..=super::RETRIES {
            if cancelled.load(Ordering::SeqCst) {
                return Err(CacheError::Cancelled);
            }
            match super::download_once(&source.url, source.bytes, &partial, cancelled) {
                Ok(()) => {
                    let (sha256, sha1, md5) = file_hashes(&partial)?;
                    if !source.accepts_acquired_bytes(&sha256, &sha1, &md5) {
                        fs::remove_file(&partial)?;
                        return Err(CacheError::Integrity(
                            "catalog checksum differs from source",
                        ));
                    }
                    let sha256 = digest_hex(&sha256);
                    let lock = SourceLock {
                        id: source.id.clone(),
                        provider: source.provider,
                        release: source.release.clone(),
                        url: source.url.clone(),
                        sha256,
                        bytes: source.bytes,
                        native_resolution: source.native_resolution.clone(),
                        crs: source.crs.clone(),
                        vertical_datum: source.vertical_datum.clone(),
                        license_reference: source.license_reference.clone(),
                    };
                    let destination = self.object_path(&lock)?;
                    match fs::hard_link(&partial, &destination) {
                        Ok(()) => {
                            fs::remove_file(&partial)?;
                            self.remember_known(source, &lock)?;
                            return Ok(lock);
                        }
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            if self.is_verified(&lock)? {
                                fs::remove_file(&partial)?;
                                self.remember_known(source, &lock)?;
                                return Ok(lock);
                            }
                            return Err(CacheError::Io(error));
                        }
                        Err(error) => return Err(CacheError::Io(error)),
                    }
                }
                Err(CacheError::Cancelled) => return Err(CacheError::Cancelled),
                Err(error) if attempt < super::RETRIES => {
                    thread::sleep(Duration::from_millis(200 * (1_u64 << attempt)));
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("retry loop always returns")
    }

    pub fn known_lock(&self, id: &str) -> Result<Option<SourceLock>, CacheError> {
        let path = self.known_path_for_id(id);
        let bytes = match read_lock(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(CacheError::Io(error)),
        };
        let lock = serde_json::from_slice::<SourceLock>(&bytes)
            .map_err(|_| CacheError::Integrity("known source lock is invalid"))?;
        let intact = lock.id == id && self.is_verified(&lock)?;
        Ok(intact.then_some(lock))
    }

    pub(super) fn cached_known(
        &self,
        source: &KnownSource,
    ) -> Result<Option<SourceLock>, CacheError> {
        // Fixed-checksum sources are checked against their catalog digest;
        // WorldCover's explicit first-acquisition policy pins to this lock.
        let path = self.known_path(source);
        let bytes = match read_lock(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return self.recover_known(source);
            }
            Err(error) => return Err(CacheError::Io(error)),
        };
        let lock = serde_json::from_slice::<SourceLock>(&bytes)
            .map_err(|_| CacheError::Integrity("known source lock is invalid"))?;
        if !same_source(source, &lock) || lock.validate().is_err() {
            return Ok(None);
        }
        let object = self.object_path(&lock)?;
        if !fs::metadata(&object)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == lock.bytes)
        {
            return Ok(None);
        }
        let (sha256, sha1, md5) = file_hashes(&object)?;
        if digest_hex(&sha256) != lock.sha256
            || !source.accepts_acquired_bytes(&sha256, &sha1, &md5)
        {
            return Ok(None);
        }
        Ok(Some(lock))
    }

    pub(super) fn remember_known(
        &self,
        source: &KnownSource,
        lock: &SourceLock,
    ) -> Result<(), CacheError> {
        let destination = self.known_path(source);
        let temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec(lock)
            .map_err(|_| CacheError::Integrity("known source lock cannot be encoded"))?;
        fs::write(&temporary, bytes)?;
        match fs::hard_link(&temporary, &destination) {
            Ok(()) => fs::remove_file(&temporary)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(&temporary)?;
                if self.cached_known(source)?.is_none() {
                    return Err(CacheError::Integrity("known source lock conflicts"));
                }
            }
            Err(error) => return Err(CacheError::Io(error)),
        }
        Ok(())
    }

    fn known_path(&self, source: &KnownSource) -> std::path::PathBuf {
        self.known_path_for_id(&source.id)
    }

    fn known_path_for_id(&self, id: &str) -> std::path::PathBuf {
        self.root
            .join("known")
            .join(format!("{:x}.json", Sha256::digest(id.as_bytes())))
    }

    fn recover_known(&self, source: &KnownSource) -> Result<Option<SourceLock>, CacheError> {
        for entry in fs::read_dir(self.root.join("objects"))? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() || metadata.len() != source.bytes {
                continue;
            }
            let (sha256, sha1, md5) = file_hashes(&entry.path())?;
            if !source.expected_checksum.matches(&sha256, &sha1, &md5) {
                continue;
            }
            let digest = digest_hex(&sha256);
            if entry.file_name().to_str() != Some(digest.as_str()) {
                continue;
            }
            let lock = SourceLock {
                id: source.id.clone(),
                provider: source.provider,
                release: source.release.clone(),
                url: source.url.clone(),
                sha256: digest_hex(&sha256),
                bytes: source.bytes,
                native_resolution: source.native_resolution.clone(),
                crs: source.crs.clone(),
                vertical_datum: source.vertical_datum.clone(),
                license_reference: source.license_reference.clone(),
            };
            self.remember_known(source, &lock)?;
            return Ok(Some(lock));
        }
        Ok(None)
    }
}

const MAX_KNOWN_LOCK_BYTES: u64 = 64 * 1024;

fn read_lock(path: &std::path::Path) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_KNOWN_LOCK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "known source lock exceeds 64 KiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_KNOWN_LOCK_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_KNOWN_LOCK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "known source lock exceeds 64 KiB",
        ));
    }
    Ok(bytes)
}

fn same_source(source: &KnownSource, lock: &SourceLock) -> bool {
    source.id == lock.id
        && source.provider == lock.provider
        && source.release == lock.release
        && source.url == lock.url
        && source.bytes == lock.bytes
        && source.native_resolution == lock.native_resolution
        && source.crs == lock.crs
        && source.vertical_datum == lock.vertical_datum
        && source.license_reference == lock.license_reference
}

#[cfg(test)]
mod tests;
