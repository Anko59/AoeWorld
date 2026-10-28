use super::overlay::{attach_in, default_base};
use std::fs;

#[test]
fn private_overlay_refreshes_generated_links_and_never_replaces_an_occupied_file() {
    let root = tempfile::tempdir().unwrap();
    let packs = root.path();
    let base = packs.join("trial");
    let de = packs.join("de");
    let other = packs.join("other");
    for p in [&base, &de, &other] {
        fs::create_dir(p).unwrap();
        fs::write(p.join("manifest.json"), "{}").unwrap();
    }
    attach_in(packs, &base, &de).unwrap();
    attach_in(packs, &base, &de).unwrap();
    assert_eq!(base.join("buildings").canonicalize().unwrap(), de);
    attach_in(packs, &base, &other).unwrap();
    assert_eq!(base.join("buildings").canonicalize().unwrap(), other);
    assert!(
        de.join("manifest.json").is_file(),
        "refresh preserves the previous pack"
    );
    assert!(attach_in(packs, &base, &base).is_err());
    fs::write(other.join("buildings"), "keep").unwrap();
    assert!(attach_in(packs, &other, &de).is_err());
    assert_eq!(fs::read_to_string(other.join("buildings")).unwrap(), "keep");
    assert!(attach_in(&base, &base, &de).is_err());
}

#[test]
fn an_import_attaches_only_to_the_unique_pack_without_an_overlay() {
    let root = tempfile::tempdir().unwrap();
    let packs = root.path();
    let base = packs.join("trial");
    let de = packs.join("de");
    for p in [&base, &de] {
        fs::create_dir(p).unwrap();
        fs::write(p.join("manifest.json"), "{}").unwrap();
    }
    assert_eq!(default_base(packs, &de).unwrap(), Some(base.clone()));
    let second = packs.join("second");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("manifest.json"), "{}").unwrap();
    assert_eq!(default_base(packs, &de).unwrap(), None);
    fs::remove_dir_all(&second).unwrap();
    attach_in(packs, &base, &de).unwrap();
    assert_eq!(default_base(packs, &de).unwrap(), None);
}
