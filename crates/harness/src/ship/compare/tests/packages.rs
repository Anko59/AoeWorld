use super::{mark, parse};
use crate::ship::compare::server;
use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

fn package(dir: &Path, hash_char: char, side: u64) {
    let hash = hash_char.to_string().repeat(64);
    let body = format!(
        r#"{{"schema_version": 1, "request": {{"center_latitude_e7": 462000000, "center_longitude_e7": 22000000, "requested_side_meters": {side}, "seed": 1}}}}"#
    );
    std::fs::write(dir.join(format!("{hash}.json")), body).expect("package");
}

#[test]
fn two_packages_for_one_map_are_refused_with_their_hashes_and_dates() {
    let dir = tempfile::tempdir().expect("dir");
    package(dir.path(), 'a', 1_200_000);
    package(dir.path(), 'b', 1_200_000);
    let marks = parse(&[mark("x")]).expect("file");
    let error = server::resolve_maps(dir.path(), &marks.maps, &["france"]).expect_err("ambiguous");
    let text = error.to_string();
    assert!(
        text.contains("several stored packages for map `france`"),
        "{text}"
    );
    assert!(
        text.contains(&"a".repeat(64)) && text.contains(&"b".repeat(64)),
        "{text}"
    );
    assert!(
        text.contains("request") && text.contains("modified"),
        "{text}"
    );
    assert!(text.contains("directory holding one"), "{text}");
}

#[test]
fn a_package_for_another_request_does_not_make_a_map_ambiguous() {
    let dir = tempfile::tempdir().expect("dir");
    package(dir.path(), 'a', 1_200_000);
    package(dir.path(), 'b', 256);
    let marks = parse(&[mark("x")]).expect("file");
    let found = server::resolve_maps(dir.path(), &marks.maps, &["france"]).expect("one match");
    assert_eq!(found["france"], "a".repeat(64));
}

#[test]
fn file_dates_are_utc_calendar_days() {
    let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(server::utc_date(at), "2023-11-14");
    assert_eq!(server::utc_date(UNIX_EPOCH), "1970-01-01");
}
