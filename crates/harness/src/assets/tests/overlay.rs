use super::overlay::attach_in;
use std::fs;

#[test]
fn private_overlay_is_idempotent_and_never_replaces_an_occupied_path() {
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
    assert!(attach_in(packs, &base, &other).is_err());
    assert!(attach_in(packs, &base, &base).is_err());
    fs::write(other.join("buildings"), "keep").unwrap();
    assert!(attach_in(packs, &other, &de).is_err());
    assert_eq!(fs::read_to_string(other.join("buildings")).unwrap(), "keep");
    assert!(attach_in(&base, &base, &de).is_err());
}
