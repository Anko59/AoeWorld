use super::{MAPS, file, mark, parse};
use crate::ship::compare::bookmarks::{Bookmarks, MAX_BOOKMARKS, MAX_VIEWS, Renderer};
use std::path::Path;

#[test]
fn a_good_bookmarks_file_parses_with_defaults() {
    let marks = parse(&[mark("alps")]).expect("good file");
    let alps = &marks.bookmarks[0];
    assert_eq!(alps.renderer, Renderer::Webgpu);
    assert!(alps.verified);
    assert_eq!(alps.note, None);
}

#[test]
fn bad_bookmarks_are_refused_with_the_reason() {
    let cases = [
        (
            r#"{"name": "a", "map": "france", "x": 1, "y": 1, "zoom": 1, "colour": 3}"#,
            "unknown field",
        ),
        (
            r#"{"name": "a", "map": "mars", "x": 1, "y": 1, "zoom": 1}"#,
            "map `mars`",
        ),
        (
            r#"{"name": "a", "map": "france", "x": 1, "y": 1, "zoom": 9}"#,
            "zoom 9",
        ),
        (
            r#"{"name": "a", "map": "france", "x": -1, "y": 1, "zoom": 1}"#,
            "x and y",
        ),
        (
            r#"{"name": "A b", "map": "france", "x": 1, "y": 1, "zoom": 1}"#,
            "bookmark name",
        ),
        (
            r#"{"name": "a", "map": "france", "x": 1, "y": 1, "zoom": 1, "renderer": "vulkan"}"#,
            "vulkan",
        ),
    ];
    for (entry, expected) in cases {
        let error = parse(&[entry.to_owned()]).expect_err(entry);
        assert!(error.contains(expected), "{entry}: {error}");
    }
    let twice = parse(&[mark("a"), mark("a")]).expect_err("duplicate");
    assert!(twice.contains("appears twice"), "{twice}");
    assert!(Bookmarks::parse(&format!(r#"{{{MAPS}, "bookmarks": []}}"#)).is_err());
    assert!(Bookmarks::parse(r#"{"bookmarks": []}"#).is_err());
}

#[test]
fn too_many_bookmarks_are_refused() {
    let many: Vec<String> = (0..=MAX_BOOKMARKS)
        .map(|i| mark(&format!("v{i}")))
        .collect();
    let error = parse(&many).expect_err("201 bookmarks");
    assert!(error.contains("at most"), "{error}");
    let limit: Vec<String> = (0..MAX_BOOKMARKS).map(|i| mark(&format!("v{i}"))).collect();
    assert!(parse(&limit).is_ok());
}

#[test]
fn views_are_selected_by_name_in_the_order_given() {
    let unverified =
        r#"{"name": "later", "map": "france", "x": 1, "y": 1, "zoom": 1, "verified": false}"#;
    let marks = parse(&[mark("a"), mark("b"), unverified.to_owned(), mark("c")]).expect("file");
    let names = |views: &str| -> Vec<String> {
        marks
            .select(Some(views))
            .expect("select")
            .iter()
            .map(|m| m.name.clone())
            .collect()
    };
    assert_eq!(names("c,a"), ["c", "a"]);
    assert_eq!(names(" b , b "), ["b"]);
    assert_eq!(names("later"), ["later"], "an unverified view can be named");
    let all: Vec<String> = marks
        .select(None)
        .expect("all")
        .iter()
        .map(|m| m.name.clone())
        .collect();
    assert_eq!(
        all,
        ["a", "b", "c"],
        "unverified views are skipped by default"
    );
    let error = marks.select(Some("a,nope")).expect_err("unknown");
    assert!(
        error.contains("no bookmark `nope`") && error.contains("a, b, later, c"),
        "{error}"
    );
}

#[test]
fn a_run_is_bounded_to_forty_views() {
    let many: Vec<String> = (0..=MAX_VIEWS).map(|i| mark(&format!("v{i}"))).collect();
    let marks = parse(&many).expect("file");
    let error = marks.select(None).expect_err("41 views");
    assert!(error.contains("at most 40"), "{error}");
    let some: Vec<String> = (0..MAX_VIEWS).map(|i| format!("v{i}")).collect();
    assert_eq!(
        marks.select(Some(&some.join(","))).expect("40").len(),
        MAX_VIEWS
    );
    let only_unverified = Bookmarks::parse(&file(&[
        r#"{"name": "a", "map": "france", "x": 1, "y": 1, "zoom": 1, "verified": false}"#.into(),
    ]))
    .expect("file");
    assert!(only_unverified.select(None).is_err());
}

#[test]
fn the_checked_in_bookmarks_are_valid() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../qa/bookmarks.json");
    let text = std::fs::read_to_string(&path).expect("qa/bookmarks.json");
    let marks = Bookmarks::parse(&text).expect("valid");
    assert!(marks.select(None).expect("verified views").len() >= 3);
}
