use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

#[test]
fn cancellation_before_manifest_never_publishes_a_package() {
    let root = test_directory();
    let map = prepared_map();
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        map.write_directory_with_cancel_check(&root, &|| {
            calls.fetch_add(1, Ordering::SeqCst) > 0
        }),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
    assert!(
        !root
            .join(format!("{}.json", map.package.content_hash_hex()))
            .exists()
    );
    assert!(
        root.join(page_file(
            &map.package.content_hash_hex(),
            PageKey {
                layer: DirectoryLayer::Elevation,
                level: 0,
                x: 0,
                y: 0,
            }
        ))
        .exists()
    );
    map.write_directory_with_cancellation(&root, &AtomicBool::new(false))
        .unwrap();
    GeneratedMap::verify_directory(&root, &map.package.content_hash_hex()).unwrap();
    fs::remove_dir_all(root).unwrap();
}
