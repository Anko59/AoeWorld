use super::{CacheError, SourceCache, directory_bytes};
use crate::KnownSource;
use std::collections::BTreeSet;

/// A bounded, offline cache estimate for an allowlisted acquisition batch.
/// Cached static sources are hash-verified; dynamic catalog entries are
/// recognized through their recorded provider-verified lock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcquisitionEstimate {
    pub source_count: usize,
    pub cached_bytes: u64,
    pub download_bytes: u64,
}

impl SourceCache {
    /// Estimates a complete allowlisted batch before any source transfer is
    /// started. It rejects duplicates and a total transfer beyond the job or
    /// cache budgets instead of beginning a partial acquisition.
    pub fn estimate_known_acquisition(
        &self,
        sources: &[KnownSource],
    ) -> Result<AcquisitionEstimate, CacheError> {
        let mut identifiers = BTreeSet::new();
        let mut cached_bytes = 0_u64;
        let mut download_bytes = 0_u64;
        for source in sources {
            if source.id.is_empty()
                || source.bytes == 0
                || !source.provider.permits(&source.url)
                || !source.has_valid_acquisition_policy()
                || !identifiers.insert(source.id.as_str())
            {
                return Err(CacheError::InvalidLock(
                    "acquisition estimate has an invalid or duplicate source",
                ));
            }
            let cached = if let Some(lock) = source.cache_lock() {
                self.is_verified(&lock)?
            } else {
                self.known_lock(&source.id)?.is_some_and(|lock| {
                    lock.provider == source.provider
                        && lock.release == source.release
                        && lock.url == source.url
                        && lock.bytes == source.bytes
                        && lock.native_resolution == source.native_resolution
                        && lock.crs == source.crs
                        && lock.vertical_datum == source.vertical_datum
                        && lock.license_reference == source.license_reference
                })
            };
            let target = if cached {
                &mut cached_bytes
            } else {
                &mut download_bytes
            };
            *target = target
                .checked_add(source.bytes)
                .ok_or(CacheError::Integrity("acquisition estimate overflow"))?;
        }
        if download_bytes > self.policy.job_acquisition_budget_bytes {
            return Err(CacheError::Budget(
                "estimated transfer exceeds the per-job acquisition budget",
            ));
        }
        let usage = directory_bytes(&self.root)?;
        if usage.saturating_add(download_bytes) > self.policy.cache_quota_bytes {
            return Err(CacheError::Budget(
                "estimated transfer exceeds the remaining cache quota",
            ));
        }
        Ok(AcquisitionEstimate {
            source_count: sources.len(),
            cached_bytes,
            download_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DownloadPolicy, ExpectedChecksum, Provider};
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn acquisition_estimate_separates_cached_and_transfer_bytes() {
        let root = temporary_directory();
        let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
        let source = KnownSource {
            id: "estimate-source".to_owned(),
            provider: Provider::Noaa,
            release: "test".to_owned(),
            url: "https://www.ngdc.noaa.gov/estimate.tif".to_owned(),
            bytes: 3,
            expected_checksum: ExpectedChecksum::Sha256([
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]),
            native_resolution: "test".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "test".to_owned(),
            license_reference: "test".to_owned(),
        };
        assert_eq!(
            cache
                .estimate_known_acquisition(std::slice::from_ref(&source))
                .expect("uncached estimate"),
            AcquisitionEstimate {
                source_count: 1,
                cached_bytes: 0,
                download_bytes: 3
            }
        );
        let lock = source.cache_lock().expect("static lock");
        fs::write(cache.object_path(&lock).expect("object"), b"abc").expect("object bytes");
        assert_eq!(
            cache
                .estimate_known_acquisition(&[source])
                .expect("cached estimate"),
            AcquisitionEstimate {
                source_count: 1,
                cached_bytes: 3,
                download_bytes: 0
            }
        );
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    fn static_source(bytes: u64) -> KnownSource {
        KnownSource {
            id: "estimate-budget-source".to_owned(),
            provider: Provider::Noaa,
            release: "test".to_owned(),
            url: "https://www.ngdc.noaa.gov/estimate-budget.tif".to_owned(),
            bytes,
            expected_checksum: ExpectedChecksum::Sha256([7; 32]),
            native_resolution: "test".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "test".to_owned(),
            license_reference: "test".to_owned(),
        }
    }

    #[test]
    fn acquisition_estimate_rejects_invalid_batches_before_budget_accounting() {
        let root = temporary_directory();
        let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
        let duplicate = static_source(3);
        let error = cache
            .estimate_known_acquisition(&[duplicate.clone(), duplicate])
            .expect_err("duplicate identifier");
        assert!(matches!(error, CacheError::InvalidLock(_)));
        let mut empty_id = static_source(3);
        empty_id.id.clear();
        assert!(matches!(
            cache.estimate_known_acquisition(&[empty_id]),
            Err(CacheError::InvalidLock(_))
        ));
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    #[test]
    fn acquisition_estimate_enforces_both_transfer_budgets() {
        let source = static_source(3);
        let job_limited_root = temporary_directory();
        let job_limited = SourceCache::new(
            job_limited_root.clone(),
            DownloadPolicy {
                cache_quota_bytes: 100,
                job_acquisition_budget_bytes: 2,
            },
        )
        .expect("job-limited cache");
        assert!(matches!(
            job_limited.estimate_known_acquisition(std::slice::from_ref(&source)),
            Err(CacheError::Budget(_))
        ));
        fs::remove_dir_all(job_limited_root).expect("remove job-limited cache");

        let quota_limited_root = temporary_directory();
        let quota_limited = SourceCache::new(
            quota_limited_root.clone(),
            DownloadPolicy {
                cache_quota_bytes: 2,
                job_acquisition_budget_bytes: 3,
            },
        )
        .expect("quota-limited cache");
        assert!(matches!(
            quota_limited.estimate_known_acquisition(&[source]),
            Err(CacheError::Budget(_))
        ));
        fs::remove_dir_all(quota_limited_root).expect("remove quota-limited cache");
    }

    #[test]
    fn provider_verified_dynamic_lock_counts_as_cached_transfer() {
        let root = temporary_directory();
        let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
        let source = KnownSource {
            id: "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N00E000_Map.tif".to_owned(),
            provider: Provider::EsaWorldCover,
            release: "ESA WorldCover 2021 v200".to_owned(),
            url: format!(
                "{}/ESA_WorldCover_10m_2021_v200_N00E000_Map.tif",
                "https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map"
            ),
            bytes: 3,
            expected_checksum: ExpectedChecksum::Sha256OnFirstAcquisition,
            native_resolution: "10 meters".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "not applicable".to_owned(),
            license_reference: "CC BY 4.0; ESA WorldCover attribution required".to_owned(),
        };
        let lock = crate::SourceLock {
            id: source.id.clone(),
            provider: source.provider,
            release: source.release.clone(),
            url: source.url.clone(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
            bytes: source.bytes,
            native_resolution: source.native_resolution.clone(),
            crs: source.crs.clone(),
            vertical_datum: source.vertical_datum.clone(),
            license_reference: source.license_reference.clone(),
        };
        fs::write(cache.object_path(&lock).expect("object"), b"abc").expect("object bytes");
        cache.remember_known(&source, &lock).expect("known lock");
        assert_eq!(
            cache
                .estimate_known_acquisition(std::slice::from_ref(&source))
                .expect("dynamic cached estimate"),
            AcquisitionEstimate {
                source_count: 1,
                cached_bytes: 3,
                download_bytes: 0
            }
        );
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    fn temporary_directory() -> std::path::PathBuf {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("aoe-geodata-estimate-{nanos}-{serial}"))
    }
}
