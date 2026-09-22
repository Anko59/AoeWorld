use super::{
    CacheError, SourceCache, SourceLock,
    digest::{digest_hex, file_hashes},
};
use crate::KnownSource;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
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
        let bytes = match fs::read(path) {
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
        let bytes = match fs::read(path) {
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
mod tests {
    use super::*;

    #[test]
    fn recorded_lock_is_usable_when_the_catalog_is_offline() {
        let root = std::env::temp_dir().join(format!("aoe-known-lock-{}", std::process::id()));
        let cache =
            SourceCache::new(root.clone(), super::super::DownloadPolicy::default()).expect("cache");
        let lock = SourceLock {
            id: "offline-source".to_owned(),
            provider: super::super::Provider::Noaa,
            release: "test".to_owned(),
            url: "https://www.ngdc.noaa.gov/source.tif".to_owned(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
            bytes: 3,
            native_resolution: "test".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "test".to_owned(),
            license_reference: "test".to_owned(),
        };
        fs::write(cache.object_path(&lock).expect("object"), b"abc").expect("object bytes");
        fs::write(
            cache.known_path_for_id(&lock.id),
            serde_json::to_vec(&lock).expect("lock JSON"),
        )
        .expect("recorded lock");
        assert_eq!(
            cache.known_lock(&lock.id).expect("cached lock"),
            Some(lock.clone())
        );
        fs::write(cache.object_path(&lock).expect("object"), b"abd").expect("tamper object");
        assert_eq!(cache.known_lock(&lock.id).expect("tampered lock"), None);
        fs::remove_dir_all(root).expect("remove temporary cache");
    }
}
