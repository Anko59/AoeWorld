use super::{mark, parse};
use crate::ship::compare::{
    config::{self, Config, Source},
    server,
};
use std::path::Path;

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

#[test]
fn settings_default_to_building_both_sides_from_the_checkout() {
    let root = Path::new("/work/repo");
    let config = Config::from_env(root, &env(&[])).expect("defaults");
    assert_eq!(
        (config.before.clone(), config.after.clone()),
        (Source::Build, Source::Build)
    );
    assert_eq!(config.bookmarks, root.join("qa/bookmarks.json"));
    assert_eq!(
        config.before_packages,
        root.join("local-assets/map-packages")
    );
    assert_eq!((config.view_seconds, config.budget_minutes), (90, 40));
    let set = Config::from_env(
        root,
        &env(&[
            ("COMPARE_BEFORE", "http://127.0.0.1:8086/"),
            ("COMPARE_AFTER", "build"),
            ("COMPARE_AFTER_PACKAGES", ".cache/other"),
            ("COMPARE_VIEWS", "a,b"),
        ]),
    )
    .expect("set");
    assert_eq!(set.before, Source::Url("http://127.0.0.1:8086/".into()));
    assert_eq!(set.after_packages, root.join(".cache/other"));
    assert_eq!(set.views.as_deref(), Some("a,b"));
}

#[test]
fn running_apps_must_be_plain_loopback_urls_and_numbers_bounded() {
    let root = Path::new("/work/repo");
    for bad in [
        "http://example.com:80/",
        "https://127.0.0.1:8080/",
        "http://127.0.0.1/",
        "http://u:p@127.0.0.1:80/",
        "http://127.0.0.1:8080/x",
        "http://127.0.0.1:8080/?a=1",
        "nonsense",
    ] {
        assert!(
            Config::from_env(root, &env(&[("COMPARE_BEFORE", bad)])).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        server::running_url("X", "http://localhost:81").expect("ok"),
        "http://localhost:81/"
    );
    for bad in ["0", "9", "601", "x", "-1"] {
        assert!(
            Config::from_env(root, &env(&[("COMPARE_VIEW_TIMEOUT_SECONDS", bad)])).is_err(),
            "{bad}"
        );
    }
    assert!(Config::from_env(root, &env(&[("COMPARE_BUDGET_MINUTES", "241")])).is_err());
}

#[test]
fn the_output_stays_below_the_cache_directory() {
    let root = Path::new("/work/repo");
    assert!(config::output_directory(root, Path::new(".cache/compare/x")).is_ok());
    for bad in ["x", ".cache", ".cache/../etc", "/tmp/out", "../.cache/x"] {
        assert!(
            config::output_directory(root, Path::new(bad)).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        config::branch_directory("harness/make-compare"),
        "harness-make-compare"
    );
    assert_eq!(config::branch_directory("HEAD"), "HEAD");
    assert_eq!(config::branch_directory(".."), "detached");
}

fn package(dir: &Path, hash_char: char, latitude: i64) {
    let hash = hash_char.to_string().repeat(64);
    let body = format!(
        r#"{{"schema_version": 1, "request": {{"center_latitude_e7": {latitude}, "center_longitude_e7": 22000000, "requested_side_meters": 1200000, "seed": 1}}}}"#
    );
    std::fs::write(dir.join(format!("{hash}.json")), body).expect("package");
}

#[test]
fn a_map_name_resolves_to_the_package_with_its_request() {
    let dir = tempfile::tempdir().expect("dir");
    package(dir.path(), 'a', 462_000_000);
    package(dir.path(), 'b', 100);
    std::fs::write(dir.path().join("notes.json"), "{}").expect("noise");
    let marks = parse(&[mark("x")]).expect("file");
    let found = server::resolve_maps(dir.path(), &marks.maps, &["france"]).expect("resolved");
    assert_eq!(found["france"], "a".repeat(64));
    let empty = tempfile::tempdir().expect("dir");
    let error = server::resolve_maps(empty.path(), &marks.maps, &["france"]).expect_err("none");
    assert!(
        error
            .to_string()
            .contains("no stored package for map `france`"),
        "{error}"
    );
}

#[test]
fn the_asset_pack_is_the_only_local_one_or_the_one_named() {
    let root = tempfile::tempdir().expect("dir");
    let packs = root.path().join("local-assets/packs");
    let error = config::asset_pack(root.path(), None).expect_err("no packs");
    assert!(error.contains("no local asset pack"), "{error}");
    for name in ["aaa", "bbb"] {
        std::fs::create_dir_all(packs.join(name)).expect("pack");
        std::fs::write(packs.join(name).join("manifest.json"), "{}").expect("manifest");
    }
    std::fs::create_dir_all(packs.join("no-manifest")).expect("noise");
    let several = config::asset_pack(root.path(), None).expect_err("two packs");
    assert!(several.contains("COMPARE_ASSET_PACK"), "{several}");
    assert_eq!(
        config::asset_pack(root.path(), Some("bbb")).expect("named"),
        "bbb"
    );
    for bad in ["missing", "../x", "a/b", ".", "no-manifest"] {
        assert!(config::asset_pack(root.path(), Some(bad)).is_err(), "{bad}");
    }
    std::fs::remove_dir_all(packs.join("bbb")).expect("remove");
    assert_eq!(
        config::asset_pack(root.path(), None).expect("the only pack"),
        "aaa"
    );
}
