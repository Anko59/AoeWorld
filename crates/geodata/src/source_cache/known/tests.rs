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

fn fixture(name: &str, policy: super::super::DownloadPolicy) -> (SourceCache, KnownSource) {
    let root = std::env::temp_dir().join(format!("aoe-known-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let cache = SourceCache::new(root, policy).unwrap();
    let source = KnownSource {
        id: name.to_owned(),
        provider: super::super::Provider::Noaa,
        release: "fixture".to_owned(),
        url: "https://www.ngdc.noaa.gov/fixture.tif".to_owned(),
        expected_checksum: crate::ExpectedChecksum::Sha256(Sha256::digest(b"abc").into()),
        bytes: 3,
        native_resolution: "fixture".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "fixture".to_owned(),
        license_reference: "fixture".to_owned(),
    };
    (cache, source)
}
fn partial(cache: &SourceCache, source: &KnownSource) -> std::path::PathBuf {
    cache.root.join("partial").join(format!(
        "known-{:x}.part",
        Sha256::digest(source.id.as_bytes())
    ))
}

#[test]
fn completed_partial_is_verified_published_and_reused_below_source_transfer_budget() {
    let (cache, source) = fixture("complete", super::super::DownloadPolicy::default());
    let path = partial(&cache, &source);
    fs::write(&path, b"abc").unwrap();
    let lock = cache
        .acquire_known(&source, &AtomicBool::new(false))
        .unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read(cache.object_path(&lock).unwrap()).unwrap(), b"abc");
    assert_eq!(cache.known_lock(&source.id).unwrap(), Some(lock.clone()));
    let limited = SourceCache::new(
        cache.root.clone(),
        super::super::DownloadPolicy {
            job_acquisition_budget_bytes: 1,
            cache_quota_bytes: 1,
        },
    )
    .unwrap();
    assert_eq!(
        limited
            .acquire_known(&source, &AtomicBool::new(true))
            .unwrap(),
        lock
    );
    fs::remove_dir_all(cache.root).unwrap();
}

#[test]
fn checksum_failure_removes_complete_partial_without_publishing() {
    let (cache, source) = fixture("bad-checksum", super::super::DownloadPolicy::default());
    let path = partial(&cache, &source);
    fs::write(&path, b"bad").unwrap();
    assert!(matches!(
        cache.acquire_known(&source, &AtomicBool::new(false)),
        Err(CacheError::Integrity(_))
    ));
    assert!(!path.exists());
    assert!(cache.known_lock(&source.id).unwrap().is_none());
    assert_eq!(fs::read_dir(cache.root.join("objects")).unwrap().count(), 0);
    fs::remove_dir_all(cache.root).unwrap();
}

#[test]
fn recovery_accepts_only_canonical_objects_and_validates_record_identity() {
    let (cache, source) = fixture("recover", super::super::DownloadPolicy::default());
    let wrong = cache.root.join("objects/not-a-content-address");
    fs::write(&wrong, b"abc").unwrap();
    assert!(matches!(
        cache.acquire_known(&source, &AtomicBool::new(true)),
        Err(CacheError::Cancelled)
    ));
    assert!(cache.known_lock(&source.id).unwrap().is_none());
    let canonical = cache
        .root
        .join("objects")
        .join(format!("{:x}", Sha256::digest(b"abc")));
    fs::rename(wrong, &canonical).unwrap();
    let lock = cache
        .acquire_known(&source, &AtomicBool::new(true))
        .unwrap();
    assert_eq!(cache.object_path(&lock).unwrap(), canonical);
    cache.remember_known(&source, &lock).unwrap();
    let mut changed = source.clone();
    changed.release = "other".to_owned();
    assert!(cache.cached_known(&changed).unwrap().is_none());
    assert!(cache.remember_known(&changed, &lock).is_err());
    fs::write(
        cache.known_path_for_id(&source.id),
        vec![b' '; MAX_KNOWN_LOCK_BYTES as usize + 1],
    )
    .unwrap();
    assert!(
        matches!(cache.known_lock(&source.id), Err(CacheError::Io(error)) if error.kind() == io::ErrorKind::InvalidData)
    );
    assert!(cache.cached_known(&source).is_err());
    fs::write(cache.known_path_for_id(&source.id), b"{").unwrap();
    assert!(matches!(
        cache.known_lock(&source.id),
        Err(CacheError::Integrity(_))
    ));
    fs::remove_dir_all(cache.root).unwrap();
}

#[test]
fn metadata_budget_and_cancellation_fail_before_download() {
    let (cache, mut source) = fixture(
        "budget",
        super::super::DownloadPolicy {
            job_acquisition_budget_bytes: 2,
            cache_quota_bytes: 1024,
        },
    );
    assert!(matches!(
        cache.acquire_known(&source, &AtomicBool::new(false)),
        Err(CacheError::Budget(_))
    ));
    let quota = SourceCache::new(
        cache.root.clone(),
        super::super::DownloadPolicy {
            job_acquisition_budget_bytes: 3,
            cache_quota_bytes: 2,
        },
    )
    .unwrap();
    assert!(matches!(
        quota.acquire_known(&source, &AtomicBool::new(false)),
        Err(CacheError::Budget(_))
    ));
    source.url = "https://example.invalid/fixture".into();
    assert!(matches!(
        cache.acquire_known(&source, &AtomicBool::new(false)),
        Err(CacheError::InvalidLock(_))
    ));
    fs::remove_dir_all(cache.root).unwrap();
}
