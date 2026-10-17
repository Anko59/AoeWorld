use super::{commit, repository};
use crate::devserver::revision::{expected, fingerprint};

#[test]
fn the_fingerprint_follows_map_code_and_ignores_everything_else() {
    let (_directory, root) = repository();
    let first = commit(&root, "crates/map/src/lib.rs", "fn a() {}");
    let before = fingerprint(&root, &first).expect("fingerprint");
    let unrelated = commit(&root, "docs/note.md", "dev moved");
    assert_eq!(fingerprint(&root, &unrelated).expect("fingerprint"), before);
    let changed = commit(&root, "crates/map/src/lib.rs", "fn b() {}");
    assert_ne!(fingerprint(&root, &changed).expect("fingerprint"), before);
    let worker = commit(&root, "crates/geodata/src/main.rs", "fn main() {}");
    assert_ne!(
        fingerprint(&root, &worker).expect("fingerprint"),
        fingerprint(&root, &changed).expect("fingerprint")
    );
}

#[test]
fn the_expected_revision_is_origin_dev_and_moves_with_it() {
    let (_directory, root) = repository();
    let first = expected(&root, None).expect("first");
    let second = commit(&root, "file.txt", "more");
    assert_ne!(first, second);
    assert_eq!(expected(&root, None).expect("second"), second);
    assert_eq!(expected(&root, Some(&first)).expect("pinned"), first);
    assert!(expected(&root, Some("no-such-revision")).is_err());
}
