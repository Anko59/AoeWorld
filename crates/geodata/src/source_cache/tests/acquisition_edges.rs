use super::*;

#[test]
fn source_cache_validates_hydrosheds_locks_and_nonzero_budgets() {
    let mut source = lock();
    source.provider = Provider::HydroSheds;
    source.url = "https://data.hydrosheds.org/example.tif".to_owned();
    assert!(source.validate().is_ok());

    source.crs.clear();
    assert!(matches!(
        source.validate(),
        Err(CacheError::InvalidLock("required metadata is missing"))
    ));

    let root = temporary_directory();
    for policy in [
        DownloadPolicy {
            cache_quota_bytes: 0,
            job_acquisition_budget_bytes: 1,
        },
        DownloadPolicy {
            cache_quota_bytes: 1,
            job_acquisition_budget_bytes: 0,
        },
    ] {
        assert!(matches!(
            SourceCache::new(root.clone(), policy),
            Err(CacheError::InvalidLock("cache budgets must be nonzero"))
        ));
    }
}

#[test]
fn cache_transport_rejects_unexpected_status_and_invalid_resume_range() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let partial = root.join("payload.part");
    let cancelled = AtomicBool::new(false);

    let server = serve(201, &[], b"created".to_vec());
    assert!(matches!(
        download_once(&server.url, 3, &partial, &cancelled),
        Err(CacheError::Download(message)) if message.contains("201")
    ));
    drop(server);

    fs::write(&partial, b"ab").expect("partial payload");
    let server = serve(206, &[("Content-Range", "bytes 1-2/3")], b"c".to_vec());
    assert!(matches!(
        download_once(&server.url, 3, &partial, &cancelled),
        Err(CacheError::Download(message)) if message.contains("invalid ranged response")
    ));
    assert!(!partial.exists(), "invalid range must discard the partial");
    drop(server);
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn source_cache_reports_wrong_length_and_unexpected_object_io() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let source = lock();
    let wrong_length = root.join("wrong-length");
    fs::write(&wrong_length, b"long").expect("wrong-length source");
    assert!(matches!(
        verify_file(&wrong_length, &source),
        Err(CacheError::Integrity(
            "file length differs from source lock"
        ))
    ));

    let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
    fs::remove_dir(root.join("objects")).expect("remove objects directory");
    fs::write(root.join("objects"), b"not a directory").expect("replace objects directory");
    assert!(matches!(cache.is_verified(&source), Err(CacheError::Io(_))));
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[cfg(unix)]
#[test]
fn content_addressed_cache_rejects_symlink_objects() {
    use std::os::unix::fs::symlink;

    let root = temporary_directory();
    let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
    let source = lock();
    let target = root.join("target");
    fs::write(&target, b"abc").expect("target bytes");
    symlink(target, cache.object_path(&source).expect("object path")).expect("object symlink");
    assert!(!cache.is_verified(&source).expect("symlink is invalid"));
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn directory_byte_accounting_recurses_through_cache_subdirectories() {
    let root = temporary_directory();
    let nested = root.join("objects/nested");
    fs::create_dir_all(&nested).expect("nested cache directory");
    fs::write(nested.join("payload"), b"abc").expect("cache object");
    assert_eq!(directory_bytes(&root).expect("cache usage"), 3);
    fs::remove_dir_all(root).expect("remove temporary cache");
}
