use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

mod digest;
use digest::{digest_hex, file_hashes};

mod known;

mod estimate;
pub use estimate::AcquisitionEstimate;

pub const DEFAULT_CACHE_QUOTA_BYTES: u64 = 100 * 1024 * 1024 * 1024;
pub const DEFAULT_JOB_ACQUISITION_BUDGET_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const RETRIES: u8 = 3;
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
                    Some("data.dans.knaw.nl" | "easy.dans.knaw.nl" | "archaeology.datastations.nl")
                )
                | (Provider::HydroSheds, Some("data.hydrosheds.org"))
                | (
                    Provider::NaturalEarth,
                    Some("naciscdn.org" | "www.naturalearthdata.com")
                )
                | (
                    Provider::Copernicus,
                    Some(
                        "copernicus-dem-30m.s3.amazonaws.com"
                            | "copernicus-dem-90m.s3.amazonaws.com",
                    )
                )
                | (
                    Provider::EsaWorldCover,
                    Some("esa-worldcover.s3.eu-central-1.amazonaws.com")
                )
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
        fs::create_dir_all(root.join("known"))?;
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
    if offset == bytes {
        return Ok(());
    }
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| CacheError::Download(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(15))
        .timeout_write(Duration::from_secs(15))
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
    if offset > 0 && status == 200 {
        fs::remove_file(partial)?;
        return Err(CacheError::Download(
            "provider ignored a ranged resume request".to_owned(),
        ));
    }
    if !((offset == 0 && status == 200) || (offset > 0 && status == 206)) {
        return Err(CacheError::Download(format!(
            "unexpected HTTP status {status}"
        )));
    }
    if offset > 0 {
        let expected_range = format!("bytes {offset}-{}/{bytes}", bytes - 1);
        if response.header("Content-Range") != Some(expected_range.as_str()) {
            fs::remove_file(partial)?;
            return Err(CacheError::Download(
                "provider returned an invalid ranged response".to_owned(),
            ));
        }
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
        crate::preparation_progress::count(
            crate::preparation_progress::Phase::DownloadingSource,
            None,
            written,
            bytes,
            crate::preparation_progress::Unit::Bytes,
        );
    }
    output.sync_all()?;
    if written != bytes {
        return Err(CacheError::Download(
            "response ended before the expected size".to_owned(),
        ));
    }
    Ok(())
}
fn verify_file(path: &Path, lock: &SourceLock) -> Result<(), CacheError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != lock.bytes {
        return Err(CacheError::Integrity(
            "file length differs from source lock",
        ));
    }
    let (actual, _, _) = file_hashes(path)?;
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
mod tests;
