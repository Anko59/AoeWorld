use crate::KnownSource;
use md5::Md5;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

pub const DEFAULT_CACHE_QUOTA_BYTES: u64 = 100 * 1024 * 1024 * 1024;
pub const DEFAULT_JOB_ACQUISITION_BUDGET_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const RETRIES: u8 = 3;
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Noaa,
    Zenodo,
    Dans,
    HydroSheds,
    NaturalEarth,
    Copernicus,
    EsaWorldCover,
}
impl Provider {
    pub(crate) fn permits(self, url: &str) -> bool {
        let host = url
            .strip_prefix("https://")
            .and_then(|value| value.split('/').next())
            .map(str::to_ascii_lowercase);
        matches!(
            (self, host.as_deref()),
            (
                Provider::Noaa,
                Some("www.ngdc.noaa.gov" | "www.ncei.noaa.gov")
            ) | (Provider::Zenodo, Some("zenodo.org" | "sandbox.zenodo.org"))
                | (
                    Provider::Dans,
                    Some("data.dans.knaw.nl" | "easy.dans.knaw.nl")
                )
                | (Provider::HydroSheds, Some("data.hydrosheds.org"))
                | (
                    Provider::NaturalEarth,
                    Some("naciscdn.org" | "www.naturalearthdata.com")
                )
                | (
                    Provider::Copernicus,
                    Some("copernicus-dem-30m.s3.amazonaws.com")
                )
                | (
                    Provider::EsaWorldCover,
                    Some("esa-worldcover.s3.eu-central-1.amazonaws.com")
                )
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLock {
    pub id: String,
    pub provider: Provider,
    pub release: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub native_resolution: String,
    pub crs: String,
    pub vertical_datum: String,
    pub license_reference: String,
}
impl SourceLock {
    pub fn validate(&self) -> Result<(), CacheError> {
        if self.id.is_empty()
            || self.release.is_empty()
            || self.bytes == 0
            || self.native_resolution.is_empty()
            || self.crs.is_empty()
            || self.license_reference.is_empty()
        {
            return Err(CacheError::InvalidLock("required metadata is missing"));
        }
        if !self.provider.permits(&self.url) {
            return Err(CacheError::InvalidLock(
                "URL is outside the provider allowlist",
            ));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CacheError::InvalidLock(
                "SHA-256 must be 64 hexadecimal characters",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DownloadPolicy {
    pub cache_quota_bytes: u64,
    pub job_acquisition_budget_bytes: u64,
}
impl Default for DownloadPolicy {
    fn default() -> Self {
        Self {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: DEFAULT_JOB_ACQUISITION_BUDGET_BYTES,
        }
    }
}

#[derive(Debug)]
pub struct SourceCache {
    root: PathBuf,
    policy: DownloadPolicy,
}
impl SourceCache {
    pub fn new(root: PathBuf, policy: DownloadPolicy) -> Result<Self, CacheError> {
        if policy.cache_quota_bytes == 0 || policy.job_acquisition_budget_bytes == 0 {
            return Err(CacheError::InvalidLock("cache budgets must be nonzero"));
        }
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("partial"))?;
        Ok(Self { root, policy })
    }

    pub fn object_path(&self, lock: &SourceLock) -> Result<PathBuf, CacheError> {
        lock.validate()?;
        Ok(self
            .root
            .join("objects")
            .join(lock.sha256.to_ascii_lowercase()))
    }

    pub fn offline_missing(&self, locks: &[SourceLock]) -> Result<Vec<String>, CacheError> {
        let mut missing = Vec::new();
        for lock in locks {
            if !self.is_verified(lock)? {
                missing.push(lock.id.clone());
            }
        }
        Ok(missing)
    }

    pub fn is_verified(&self, lock: &SourceLock) -> Result<bool, CacheError> {
        let path = self.object_path(lock)?;
        match verify_file(&path, lock) {
            Ok(()) => Ok(true),
            Err(CacheError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(CacheError::Integrity(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn acquire(
        &self,
        lock: &SourceLock,
        cancelled: &AtomicBool,
    ) -> Result<PathBuf, CacheError> {
        let destination = self.object_path(lock)?;
        if self.is_verified(lock)? {
            return Ok(destination);
        }
        if lock.bytes > self.policy.job_acquisition_budget_bytes {
            return Err(CacheError::Budget(
                "source exceeds the per-job acquisition budget",
            ));
        }
        let usage = directory_bytes(&self.root)?;
        if usage.saturating_add(lock.bytes) > self.policy.cache_quota_bytes {
            return Err(CacheError::Budget(
                "source exceeds the remaining cache quota",
            ));
        }
        let partial = self
            .root
            .join("partial")
            .join(format!("{}.part", lock.sha256));
        for attempt in 0..=RETRIES {
            if cancelled.load(Ordering::SeqCst) {
                return Err(CacheError::Cancelled);
            }
            match download_once(&lock.url, lock.bytes, &partial, cancelled) {
                Ok(()) => {
                    verify_file(&partial, lock)?;
                    match fs::hard_link(&partial, &destination) {
                        Ok(()) => {
                            fs::remove_file(&partial)?;
                            return Ok(destination);
                        }
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            if self.is_verified(lock)? {
                                return Ok(destination);
                            }
                            return Err(CacheError::Io(error));
                        }
                        Err(error) => return Err(CacheError::Io(error)),
                    }
                }
                Err(CacheError::Cancelled) => return Err(CacheError::Cancelled),
                Err(error) if attempt < RETRIES => {
                    thread::sleep(Duration::from_millis(200 * (1_u64 << attempt)));
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("retry loop always returns")
    }

    pub fn acquire_known(
        &self,
        source: &KnownSource,
        cancelled: &AtomicBool,
    ) -> Result<SourceLock, CacheError> {
        if source.id.is_empty() || source.bytes == 0 || !source.provider.permits(&source.url) {
            return Err(CacheError::InvalidLock("known source metadata is invalid"));
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
        let usage = directory_bytes(&self.root)?;
        let partial_bytes = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
        if usage.saturating_add(source.bytes.saturating_sub(partial_bytes))
            > self.policy.cache_quota_bytes
        {
            return Err(CacheError::Budget(
                "source exceeds the remaining cache quota",
            ));
        }
        for attempt in 0..=RETRIES {
            if cancelled.load(Ordering::SeqCst) {
                return Err(CacheError::Cancelled);
            }
            match download_once(&source.url, source.bytes, &partial, cancelled) {
                Ok(()) => {
                    let (sha256, md5) = file_hashes(&partial)?;
                    if !source.expected_checksum.matches(&sha256, &md5) {
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
                            return Ok(lock);
                        }
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            if self.is_verified(&lock)? {
                                fs::remove_file(&partial)?;
                                return Ok(lock);
                            }
                            return Err(CacheError::Io(error));
                        }
                        Err(error) => return Err(CacheError::Io(error)),
                    }
                }
                Err(CacheError::Cancelled) => return Err(CacheError::Cancelled),
                Err(error) if attempt < RETRIES => {
                    thread::sleep(Duration::from_millis(200 * (1_u64 << attempt)));
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("retry loop always returns")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("invalid source lock: {0}")]
    InvalidLock(&'static str),
    #[error("source cache I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("source download failed: {0}")]
    Download(String),
    #[error("source integrity check failed: {0}")]
    Integrity(&'static str),
    #[error("source download was cancelled")]
    Cancelled,
    #[error("source download budget is insufficient: {0}")]
    Budget(&'static str),
}
fn download_once(
    url: &str,
    bytes: u64,
    partial: &Path,
    cancelled: &AtomicBool,
) -> Result<(), CacheError> {
    let offset = fs::metadata(partial).map(|meta| meta.len()).unwrap_or(0);
    if offset > bytes {
        fs::remove_file(partial)?;
        return Err(CacheError::Integrity("partial file exceeds expected size"));
    }
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| CacheError::Download(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .build();
    let request = if offset == 0 {
        agent.get(url)
    } else {
        agent.get(url).set("Range", &format!("bytes={offset}-"))
    };
    let response = request
        .call()
        .map_err(|error| CacheError::Download(error.to_string()))?;
    let status = response.status();
    if !(status == 206 || (offset == 0 && status == 200)) {
        return Err(CacheError::Download(format!(
            "unexpected HTTP status {status}"
        )));
    }
    if offset > 0 && status == 200 {
        fs::remove_file(partial)?;
        return Err(CacheError::Download(
            "provider ignored a ranged resume request".to_owned(),
        ));
    }
    let mut output = OpenOptions::new().create(true).append(true).open(partial)?;
    let mut input = response.into_reader();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    let mut written = offset;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(CacheError::Cancelled);
        }
        let count = input.read(&mut buffer).map_err(CacheError::Io)?;
        if count == 0 {
            break;
        }
        written = written
            .checked_add(u64::try_from(count).map_err(|_| CacheError::Integrity("copy overflow"))?)
            .ok_or(CacheError::Integrity("copy overflow"))?;
        if written > bytes {
            return Err(CacheError::Integrity("response exceeds expected size"));
        }
        output.write_all(&buffer[..count])?;
    }
    output.sync_all()?;
    if written != bytes {
        return Err(CacheError::Download(
            "response ended before the expected size".to_owned(),
        ));
    }
    Ok(())
}
fn file_hashes(path: &Path) -> Result<([u8; 32], [u8; 16]), CacheError> {
    let mut file = File::open(path)?;
    let mut sha = Sha256::new();
    let mut md5 = Md5::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
        md5.update(&buffer[..count]);
    }
    Ok((sha.finalize().into(), md5.finalize().into()))
}

fn digest_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn verify_file(path: &Path, lock: &SourceLock) -> Result<(), CacheError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != lock.bytes {
        return Err(CacheError::Integrity(
            "file length differs from source lock",
        ));
    }
    let (actual, _) = file_hashes(path)?;
    let actual = digest_hex(&actual);
    if !actual.eq_ignore_ascii_case(&lock.sha256) {
        return Err(CacheError::Integrity("SHA-256 differs from source lock"));
    }
    Ok(())
}
fn directory_bytes(path: &Path) -> Result<u64, CacheError> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            total = total.saturating_add(directory_bytes(&entry.path())?);
        } else if file_type.is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn lock() -> SourceLock {
        SourceLock {
            id: "etopo-2022-60s".to_owned(),
            provider: Provider::Noaa,
            release: "ETOPO 2022 v1".to_owned(),
            url: "https://www.ngdc.noaa.gov/example.tif".to_owned(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
            bytes: 3,
            native_resolution: "60 arc-seconds".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "ice surface".to_owned(),
            license_reference: "NOAA public domain".to_owned(),
        }
    }

    #[test]
    fn source_locks_reject_unapproved_or_incomplete_sources() {
        let mut source = lock();
        assert!(source.validate().is_ok());
        source.url = "http://www.ngdc.noaa.gov/example.tif".to_owned();
        assert!(source.validate().is_err());
        source.url = "https://www.ngdc.noaa.gov/example.tif".to_owned();
        source.sha256 = "not-a-hash".to_owned();
        assert!(source.validate().is_err());
    }

    #[test]
    fn offline_reports_only_missing_or_tampered_inputs() {
        let root = temporary_directory();
        let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
        let source = lock();
        let object = cache.object_path(&source).expect("object path");
        fs::write(&object, b"abc").expect("cached object");
        assert!(
            cache
                .offline_missing(std::slice::from_ref(&source))
                .expect("offline")
                .is_empty()
        );
        fs::write(&object, b"bad").expect("tampered object");
        assert_eq!(
            cache.offline_missing(&[source]).expect("offline"),
            vec!["etopo-2022-60s"]
        );
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    #[test]
    fn source_acquisition_hashes_sha256_and_provider_md5() {
        let root = temporary_directory();
        fs::create_dir_all(&root).expect("temporary root");
        let path = root.join("source");
        fs::write(&path, b"abc").expect("source bytes");
        let (sha, md5) = file_hashes(&path).expect("digests");
        assert_eq!(
            digest_hex(&sha),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(digest_hex(&md5), "900150983cd24fb0d6963f7d28e17f72");
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    fn temporary_directory() -> PathBuf {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("aoe-geodata-{nanos}-{serial}"))
    }
}
