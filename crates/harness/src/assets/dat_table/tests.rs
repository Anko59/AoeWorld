use super::pack_ids;

#[test]
fn only_a_missing_graphics_archive_means_no_pack() {
    let holder = tempfile::tempdir().unwrap();
    let dir = holder.path().to_path_buf();
    assert!(pack_ids(&dir).unwrap().is_none());
    // A directory where the archive should be is a read error, not "no pack".
    std::fs::create_dir_all(dir.join("graphics.drs")).unwrap();
    assert!(pack_ids(&dir).is_err());
}
