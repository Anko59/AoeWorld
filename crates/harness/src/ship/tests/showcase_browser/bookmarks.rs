use super::*;

fn setup(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("qa")).unwrap();
    let marks = r#"{"maps":{"finland":{"center_latitude_e7":615000000,"center_longitude_e7":255000000,"requested_side_meters":30000}},"bookmarks":[{"name":"forest","map":"finland","x":100.5,"y":210,"zoom":0.698,"renderer":"webgl2"}]}"#;
    std::fs::write(root.join("qa/bookmarks.json"), marks).unwrap();
    let packages = root.join("local-assets/map-packages");
    std::fs::create_dir_all(&packages).unwrap();
    std::fs::write(packages.join(format!("{}.json", "a".repeat(64))),
        r#"{"request":{"center_latitude_e7":615000000,"center_longitude_e7":255000000,"requested_side_meters":30000}}"#).unwrap();
}

#[test]
fn named_bookmarks_use_compares_fragment_and_resolve_the_stored_map() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    setup(temp.path());
    mode("SHOWCASE_LEVEL", "medium");
    let scene = r#"{"kind":"browser","bookmark":"forest","caption":"Forest","seconds":2}"#;
    std::fs::write(
        temp.path().join("storyboard.json"),
        format!(r#"{{"title":"View","scenes":[{scene},{scene}]}}"#),
    )
    .unwrap();
    check(temp.path()).unwrap();
    make(temp.path()).unwrap();
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    let value: serde_json::Value = serde_json::from_str(
        log.lines()
            .find(|line| line.starts_with("{\"title\""))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        value["scenes"][0]["url"],
        "http://127.0.0.1:8080/#x=100.5&y=210&zoom=0.698&renderer=webgl2"
    );
    assert_eq!(value["scenes"][0]["view"]["map_hash"], "a".repeat(64));
    assert_eq!(value["scenes"][0]["view"]["x"], 100.5);
    assert_eq!(value["scenes"][0]["view"]["renderer"], "webgl2");
}

#[test]
fn unknown_bookmarks_and_ambiguous_packages_fail_before_tts_or_images() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    setup(temp.path());
    mode("SHOWCASE_LEVEL", "medium");
    let board = r#"{"title":"View","scenes":[{"kind":"browser","bookmark":"missing","caption":"Forest","seconds":2}]}"#;
    std::fs::write(temp.path().join("storyboard.json"), board).unwrap();
    assert!(
        check(temp.path())
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    std::fs::write(
        temp.path().join("storyboard.json"),
        board.replace("missing", "forest"),
    )
    .unwrap();
    let packages = temp.path().join("local-assets/map-packages");
    std::fs::copy(
        packages.join(format!("{}.json", "a".repeat(64))),
        packages.join(format!("{}.json", "b".repeat(64))),
    )
    .unwrap();
    assert!(
        check(temp.path())
            .unwrap_err()
            .to_string()
            .contains("several stored packages")
    );
    assert!(!temp.path().join("invocations.log").exists());
}

#[test]
fn a_bookmark_cannot_silently_replace_a_path_or_camera() {
    for path in ["/lab", "/#x=3"] {
        let text = format!(
            r#"{{"title":"View","scenes":[{{"kind":"browser","bookmark":"forest","path":"{path}","caption":"Forest","seconds":2}}]}}"#
        );
        assert!(Storyboard::parse(&text).is_err(), "{path}");
    }
    assert!(
        Storyboard::parse(&browser_board("/", "").replace("\"path\":\"/\"", "\"bookmark\":\"\""))
            .is_err()
    );
}

#[test]
fn recorded_bookmark_evidence_is_checked_before_publishing() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (failure, reason) in [
        ("map", "map"),
        ("camera", "camera"),
        ("renderer", "renderer"),
        ("unsettled", "not settled"),
    ] {
        let (_env, temp) = Environment::setup();
        setup(temp.path());
        mode("SHOWCASE_LEVEL", "medium");
        mode("SHOWCASE_VIEW_MODE", failure);
        let scene = r#"{"kind":"browser","bookmark":"forest","caption":"Forest","seconds":2}"#;
        std::fs::write(
            temp.path().join("storyboard.json"),
            format!(r#"{{"title":"View","scenes":[{scene},{scene}]}}"#),
        )
        .unwrap();
        let error = make(temp.path()).unwrap_err().to_string();
        assert!(error.contains(reason), "{failure}: {error}");
        assert!(!temp.path().join(".cache/showcase/final.webm").exists());
    }
}
